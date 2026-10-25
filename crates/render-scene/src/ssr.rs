//! Scheduled reflections add only the environment-specular lobe, currently zero in forward lighting.
use crate::frame::metadata::FrameProduct;
mod gpu;
mod metadata;
#[cfg(test)]
mod tests;
use crate::{frame::FrameUniform, ssao::Ssao};
use worldrunner_render_plan::{SceneSsao, SceneSsr};
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FeatureDeclaration, FeaturePass, FrameConfig, FrameError,
    PassDeclaration, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};

pub(crate) fn validate_settings(settings: &SceneSsr) -> Result<(), FrameError> {
    settings.validate().map_err(|_| {
        FrameError::SsrSettings("steps, thickness, cutoff or resolution scale out of range")
    })
}
pub(crate) const fn neutral_ao() -> SceneSsao {
    SceneSsao {
        sample_count: 1,
        bias_millimeters: 0,
        radius_millimeters: 1,
        intensity_micro: 0,
        horizon_angle_millidegrees: 90_000,
    }
}
pub(crate) fn geometry_declaration(declaration: &mut FeatureDeclaration) {
    let mut pass = FeaturePass::new(PassSlot::ReflectionMaterial);
    pass.color_format = Some(wgpu::TextureFormat::Rgba16Float);
    pass.depth_format = Some(wgpu::TextureFormat::Depth32Float);
    declaration.passes.push(pass);
}
fn resource(index: u32) -> ResourceRef {
    ResourceRef {
        version: index,
        ..ResourceRef::new("reflection radiance")
    }
}
#[derive(Debug)]
pub(crate) struct Ssr {
    pub settings: SceneSsr,
    material: wgpu::Texture,
    material_depth: wgpu::Texture,
    depth: wgpu::Texture,
    normals: wgpu::Texture,
    hierarchy: Vec<wgpu::Texture>,
    reflections: Vec<wgpu::Texture>,
    uniform: wgpu::Buffer,
    trace: wgpu::RenderPipeline,
    reduce: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
}
impl Ssr {
    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        settings: SceneSsr,
        screen: &mut Ssao,
    ) -> Result<Self, FrameError> {
        validate_settings(&settings)?;
        let (pipelines, group) = metadata::pipelines(device, &screen.uniform, false);
        screen.geometry.pipelines = pipelines;
        screen.geometry.group = group;
        screen.geometry.reflection = Some(metadata::pipelines(device, &screen.uniform, true).0);
        // All geometry pipelines use the same explicit group layout as the normal prepass.

        let material = metadata::material_texture(device, config.size, screen)?;
        let levels = screen.passes.depth_levels();
        let hierarchy: Vec<_> = [0, 2, 4, 6]
            .into_iter()
            .map(|index| levels[index.min(levels.len() - 1)].clone())
            .collect();
        let mut extent = if settings.resolution_scale_micro == 500_000 {
            config.size.map(|side| side.div_ceil(2))
        } else {
            config.size
        };
        let mut reflections = Vec::new();
        loop {
            reflections.push(crate::ssao::passes::texture(
                device,
                extent,
                wgpu::TextureFormat::Rgba16Float,
            ));
            if extent == [1, 1] || reflections.len() == 6 {
                break;
            }
            extent = extent.map(|side| side.div_ceil(2));
        }
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSR camera"),
            size: 160,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let trace = gpu::pipeline(device, include_str!("ssr/trace.wgsl"), 1, false);
        let reduce = gpu::pipeline(device, include_str!("ssr/reduce.wgsl"), 1, false);
        let resolve = gpu::pipeline(
            device,
            &gpu::resolve_source(reflections.len()),
            config.samples,
            true,
        );
        Ok(Self {
            settings,
            material_depth: crate::ssao::passes::texture(
                device,
                config.size,
                wgpu::TextureFormat::Depth32Float,
            ),
            material,
            depth: screen.depth.clone(),
            normals: screen.normals.clone(),
            hierarchy,
            reflections,
            uniform,
            trace,
            reduce,
            resolve,
        })
    }
    pub fn feature<'a>(
        &'a self,
        _device: &wgpu::Device,
        frame: FrameUniform,
        passes: &[PassDeclaration],
        views: &[AttachmentView],
        config: FrameConfig,
    ) -> Result<ReflectionFeature<'a>, FrameError> {
        let color = crate::water::planning::opaque_source(passes, views, config, false)?.clone();
        Ok(ReflectionFeature {
            ssr: self,
            frame,
            color,
            groups: Vec::new(),
        })
    }
}
pub(crate) struct ReflectionFeature<'a> {
    ssr: &'a Ssr,
    frame: FrameUniform,
    color: AttachmentView,
    groups: Vec<wgpu::BindGroup>,
}
impl ReflectionFeature<'_> {
    fn trace_reads(&self) -> Vec<ResourceRef> {
        let ssr = self.ssr;
        let mut reads = vec![
            ResourceRef::new("opaque snapshot color"),
            FrameProduct::normals().resource(),
            FrameProduct::material().resource(),
        ];
        for level in [0, 2, 4, 6] {
            let last = ssr
                .depth
                .width()
                .max(ssr.depth.height())
                .next_power_of_two()
                .trailing_zeros();
            reads.push(ResourceRef {
                version: level.min(last),
                ..ResourceRef::new("linear camera depth")
            });
        }
        reads.sort();
        reads.dedup();
        reads
    }
    fn schedule_metadata(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        let ssr = self.ssr;
        let material = FrameProduct::material().attachment(&ssr.material)?;
        let depth = AttachmentView::new(
            ResourceRef::new("reflection metadata depth"),
            &ssr.material_depth,
        )?;
        let mut metadata = PassDeclaration::new(PassSlot::ReflectionMaterial);
        metadata.color = Some(crate::compositor::attachment(
            material.resource(),
            &material,
            false,
        ));
        metadata.depth = Some(crate::compositor::attachment(
            depth.resource(),
            &depth,
            true,
        ));
        views.push(depth);
        passes.push(metadata);
        views.push(material);
        Ok(())
    }
    pub fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let ssr = self.ssr;
        let mut declaration = FeatureDeclaration {
            passes: Vec::new(),
            effects: vec!["SSR"],
        };
        self.schedule_metadata(passes, views)?;
        for (index, texture) in ssr.reflections.iter().enumerate() {
            let index = u32::try_from(index).expect("bounded reflection levels");
            let slot = if index == 0 {
                PassSlot::ReflectionTrace
            } else {
                PassSlot::ReflectionMip(index)
            };
            let view = AttachmentView::new(resource(index), texture)?;
            let mut pass = PassDeclaration::new(slot);
            pass.color = Some(crate::compositor::attachment(view.resource(), &view, false));
            passes.push(pass);
            views.push(view);
            let mut feature = FeaturePass::new(slot);
            feature.color_format = Some(wgpu::TextureFormat::Rgba16Float);
            feature.reads = if index == 0 {
                self.trace_reads()
            } else {
                vec![resource(index - 1)]
            };
            declaration.passes.push(feature);
        }
        let mut resolve = PassDeclaration::new(PassSlot::ScreenSpaceAfterTransparent);
        let mut color = crate::compositor::attachment(self.color.resource(), &self.color, false);
        color.load = AttachmentLoad::Load;
        resolve.color = Some(color);
        passes.push(resolve);
        let mut feature = FeaturePass::new(PassSlot::ScreenSpaceAfterTransparent);
        feature.samples = self.color.texture().sample_count();
        feature.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        feature.reads = (0..u32::try_from(ssr.reflections.len()).expect("bounded levels"))
            .map(resource)
            .collect();
        feature.reads.extend([
            FrameProduct::material().resource(),
            FrameProduct::normals().resource(),
            FrameProduct::opaque_depth(self.color.texture().sample_count()).resource(),
            FrameProduct::final_depth(self.color.texture().sample_count()).resource(),
        ]);
        declaration.passes.push(feature);
        Ok(declaration)
    }
}
impl PreparedFeature for ReflectionFeature<'_> {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let ssr = self.ssr;
        let snapshot = context.resource(ResourceRef::new("opaque snapshot color"))?;
        let opaque_depth = context
            .resource(FrameProduct::opaque_depth(self.color.texture().sample_count()).resource())?;
        let final_depth = context
            .resource(FrameProduct::final_depth(self.color.texture().sample_count()).resource())?;
        let mut trace_inputs = vec![
            snapshot.view().clone(),
            ssr.normals
                .create_view(&wgpu::TextureViewDescriptor::default()),
            ssr.material
                .create_view(&wgpu::TextureViewDescriptor::default()),
        ];
        trace_inputs.extend(
            ssr.hierarchy
                .iter()
                .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
        );
        self.groups = vec![gpu::group(
            context.device(),
            &ssr.trace,
            Some(&ssr.uniform),
            &trace_inputs,
        )];
        for texture in ssr.reflections.iter().take(ssr.reflections.len() - 1) {
            self.groups.push(gpu::group(
                context.device(),
                &ssr.reduce,
                None,
                &[texture.create_view(&wgpu::TextureViewDescriptor::default())],
            ));
        }
        let mut resolve_inputs = vec![
            ssr.material
                .create_view(&wgpu::TextureViewDescriptor::default()),
            ssr.normals
                .create_view(&wgpu::TextureViewDescriptor::default()),
            opaque_depth.view().clone(),
            final_depth.view().clone(),
        ];
        resolve_inputs.extend(
            ssr.reflections
                .iter()
                .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
        );
        self.groups.push(gpu::group(
            context.device(),
            &ssr.resolve,
            Some(&ssr.uniform),
            &resolve_inputs,
        ));
        Ok(())
    }
    fn commit(&mut self, context: &mut PrepareContext<'_>) {
        let projection = self.frame.view_projection * self.frame.view.inverse();
        let mut values: Vec<f32> = [projection, projection.inverse()]
            .into_iter()
            .flat_map(|matrix| matrix.to_cols_array())
            .collect();
        values.extend([
            gpu::scalar(self.ssr.settings.max_steps),
            gpu::scalar(self.ssr.settings.thickness_millimeters) / 1000.0,
            gpu::scalar(self.ssr.settings.roughness_cutoff_micro) / 1_000_000.0,
            f32::from(u8::from(self.frame.unlit)),
            gpu::scalar(self.color.size()[0]),
            gpu::scalar(self.color.size()[1]),
            if self.ssr.settings.resolution_scale_micro == 500_000 {
                2.0
            } else {
                1.0
            },
            0.0,
        ]);
        context.upload_buffer(
            &self.ssr.uniform,
            0,
            &values
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let (pipeline, index) = match context.slot {
            PassSlot::ReflectionTrace => (&self.ssr.trace, 0),
            PassSlot::ReflectionMip(index) => {
                (&self.ssr.reduce, usize::try_from(index).expect("mip index"))
            }
            PassSlot::ScreenSpaceAfterTransparent => (&self.ssr.resolve, self.groups.len() - 1),
            _ => unreachable!("reflection slot"),
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.groups[index], &[]);
        pass.draw(0..3, 0..1);
    }
}
