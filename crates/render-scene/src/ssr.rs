//! Scheduled opaque reflections with conservative final-scene depth traversal.
use crate::frame::metadata::FrameProduct;
mod final_depth;
pub(crate) mod forward;
mod gpu;
mod metadata;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod water;
use crate::{frame::FrameUniform, scene_metadata::SceneMetadata};
use worldrunner_render_plan::{SceneSsao, SceneSsr};
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FeatureDeclaration, FeaturePass, FrameConfig, FrameError,
    PassDeclaration, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};

pub(crate) fn validate_settings(settings: &SceneSsr) -> Result<(), FrameError> {
    settings
        .validate()
        .map_err(|_| FrameError::SsrSettings("invalid reflection settings"))
}
pub(crate) fn validate_extent(
    config: FrameConfig,
    settings: &SceneSsr,
    limits: &wgpu::Limits,
) -> Result<(), FrameError> {
    let width = if settings.resolution_scale_micro == 500_000 {
        config.size[0].div_ceil(2)
    } else {
        config.size[0]
    };
    if width
        .checked_mul(config.samples)
        .is_none_or(|width| width > limits.max_texture_dimension_2d)
        || config.size[1] > limits.max_texture_dimension_2d
    {
        return Err(FrameError::SsrSettings(
            "sample-packed radiance extent exceeds device texture limit",
        ));
    }
    Ok(())
}
pub(crate) const fn neutral_ao() -> SceneSsao {
    SceneSsao {
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
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
    forward_products: [wgpu::Texture; 2],
    pub environment: [wgpu::TextureView; 2],
    pub composition: worldrunner_render_lighting::reflections::Composition,
    material: wgpu::Texture,
    material_depth: wgpu::Texture,
    source_depth: wgpu::Texture,
    normals: wgpu::Texture,
    hierarchy: Vec<wgpu::Texture>,
    final_depth: final_depth::FinalDepth,
    reflections: Vec<wgpu::Texture>,
    uniform: wgpu::Buffer,
    trace: wgpu::RenderPipeline,
    reduce: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
    water_trace: wgpu::RenderPipeline,
    water_resolve: wgpu::RenderPipeline,
}
impl Ssr {
    pub(crate) fn textures(&self) -> Vec<&wgpu::Texture> {
        let mut textures = vec![&self.material, &self.material_depth];
        textures.extend(self.forward_products.iter());
        textures.extend(self.reflections.iter());
        textures.extend(self.final_depth.textures());
        textures
    }

    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        settings: SceneSsr,
        screen: &mut SceneMetadata,
    ) -> Result<Self, FrameError> {
        validate_settings(&settings)?;
        validate_extent(config, &settings, &device.limits())?;
        let environment = forward::install(device, config, screen);
        screen.geometry.reflection =
            Some(crate::scene_metadata::pipelines(device, &screen.uniform, true).0);
        // All geometry pipelines use the same explicit group layout as the normal prepass.

        let material = metadata::material_texture(device, config.size, screen)?;
        let final_depth = final_depth::FinalDepth::new(device, config.size, config.samples);
        let hierarchy = final_depth.trace_levels();
        let reflections = gpu::radiance_textures(device, config, settings.resolution_scale_micro);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSR camera"),
            size: 192,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let trace = gpu::pipeline(device, &gpu::trace_source(config.samples, false), 1, false);
        let reduce = gpu::pipeline(device, &gpu::reduce_source(config.samples), 1, false);
        let resolve = gpu::pipeline(
            device,
            &gpu::sampled_resolve_source(reflections.len(), config.samples, false),
            config.samples,
            true,
        );
        let water_trace = gpu::pipeline(device, &gpu::trace_source(config.samples, true), 1, false);
        let water_resolve = gpu::pipeline(
            device,
            &gpu::sampled_resolve_source(reflections.len(), config.samples, true),
            config.samples,
            true,
        );
        Ok(Self {
            settings,
            environment,
            forward_products: forward::products(device, config),
            composition: match settings.composition {
                worldrunner_render_plan::SceneSsrComposition::Additive => {
                    worldrunner_render_lighting::reflections::Composition::Additive
                }
                worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment => {
                    worldrunner_render_lighting::reflections::Composition::ReplaceEnvironment
                }
            },
            water_trace,
            water_resolve,
            material_depth: crate::ssao::passes::texture(
                device,
                config.size,
                wgpu::TextureFormat::Depth32Float,
            ),
            material,
            source_depth: screen.passes.depth_levels()[0].clone(),
            normals: screen.normals.clone(),
            hierarchy,
            final_depth,
            reflections,
            uniform,
            trace,
            reduce,
            resolve,
        })
    }
    pub fn resized(
        &self,
        device: &wgpu::Device,
        config: FrameConfig,
        screen: &mut SceneMetadata,
    ) -> Result<Self, FrameError> {
        let mut replacement = Self::new(device, config, self.settings, screen)?;
        replacement.environment.clone_from(&self.environment);
        replacement.composition = self.composition;
        let group = forward::group(device, screen, &replacement.environment);
        screen
            .geometry
            .forward
            .as_mut()
            .expect("forward metadata")
            .1 = group;
        Ok(replacement)
    }
    pub fn feature<'a>(
        &'a self,
        frame: &FrameUniform,
        plan: (&[PassDeclaration], &[AttachmentView]),
        config: FrameConfig,
        mut water: Option<water::Inputs>,
    ) -> Result<ReflectionFeature<'a>, FrameError> {
        let color = crate::water::planning::opaque_source(plan.0, plan.1, config, false)?.clone();
        if let Some(inputs) = &mut water {
            inputs.depth =
                crate::water::planning::opaque_source(plan.0, plan.1, config, true)?.resource();
        }
        Ok(ReflectionFeature {
            ssr: self,
            water,
            frame: *frame,
            color,
            groups: Vec::new(),
        })
    }
}
pub(crate) struct ReflectionFeature<'a> {
    ssr: &'a Ssr,
    water: Option<water::Inputs>,
    frame: FrameUniform,
    color: AttachmentView,
    groups: Vec<wgpu::BindGroup>,
}
impl ReflectionFeature<'_> {
    fn prepare_trace(&mut self, context: &PrepareContext<'_>) -> Result<(), FrameError> {
        let ssr = self.ssr;
        let snapshot = context.resource(ResourceRef::new("opaque snapshot color"))?;
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
        trace_inputs.push(self.ssr_source_depth());
        trace_inputs.push(
            context
                .resource(
                    FrameProduct::opaque_depth(self.color.texture().sample_count()).resource(),
                )?
                .view()
                .clone(),
        );
        if let Some(water) = &self.water {
            water.trace_views(context, &mut trace_inputs)?;
        }
        let trace_pipeline = if self.water.is_some() {
            &ssr.water_trace
        } else {
            &ssr.trace
        };
        self.groups = vec![gpu::group(
            context.device(),
            trace_pipeline,
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
        Ok(())
    }
    fn ssr_source_depth(&self) -> wgpu::TextureView {
        self.ssr
            .source_depth
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
    fn trace_reads(&self) -> Vec<ResourceRef> {
        let ssr = self.ssr;
        let mut reads = vec![
            ResourceRef::new("opaque snapshot color"),
            FrameProduct::normals().resource(),
            FrameProduct::material().resource(),
        ];
        if let Some(water) = &self.water {
            reads.extend(water::reads(water.depth));
        }
        reads.extend(ssr.final_depth.reads());
        reads.push(FrameProduct::linear_depth(0).resource());
        reads.push(FrameProduct::opaque_depth(self.color.texture().sample_count()).resource());
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
        forward::schedule(&ssr.forward_products, passes, views)?;
        declaration.passes.extend(ssr.final_depth.schedule(
            self.color.texture().sample_count(),
            passes,
            views,
        )?);
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
        feature.reads.extend(forward::resources());
        feature.reads.extend([
            FrameProduct::material().resource(),
            FrameProduct::normals().resource(),
            FrameProduct::opaque_depth(self.color.texture().sample_count()).resource(),
            FrameProduct::final_depth(self.color.texture().sample_count()).resource(),
        ]);
        if let Some(water) = &self.water {
            feature.reads.extend(water::reads(water.depth));
        }
        declaration.passes.push(feature);
        Ok(declaration)
    }
}
impl PreparedFeature for ReflectionFeature<'_> {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let ssr = self.ssr;
        let opaque_depth = context
            .resource(FrameProduct::opaque_depth(self.color.texture().sample_count()).resource())?;
        let final_depth = context
            .resource(FrameProduct::final_depth(self.color.texture().sample_count()).resource())?;
        self.prepare_trace(context)?;
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
        resolve_inputs.extend(
            ssr.forward_products
                .each_ref()
                .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
        );
        if let Some(water) = &self.water {
            water.resolve_views(&mut resolve_inputs);
        }
        let resolve_pipeline = if self.water.is_some() {
            &ssr.water_resolve
        } else {
            &ssr.resolve
        };
        self.groups.push(gpu::group(
            context.device(),
            resolve_pipeline,
            Some(&ssr.uniform),
            &resolve_inputs,
        ));
        self.groups
            .extend(ssr.final_depth.groups(context.device(), final_depth.view()));
        Ok(())
    }
    fn commit(&mut self, context: &mut PrepareContext<'_>) {
        let mut projection = self.frame.view_projection * self.frame.view.inverse();
        // Multiplying by inverse(view) can leave roundoff in this projection tag.
        // Derive it from clip-from-world, whose perspective row has a direction.
        projection.w_axis.w = f32::from(u8::from(
            self.frame.view_projection.row(3).truncate() == glam::Vec3::ZERO,
        ));
        let mut values: Vec<f32> = [projection, projection.inverse()]
            .into_iter()
            .flat_map(|matrix| matrix.to_cols_array())
            .collect();
        values.extend([
            gpu::scalar(self.ssr.settings.max_steps),
            gpu::scalar(self.ssr.settings.thickness_millimeters) / 1000.0,
            gpu::scalar(self.ssr.settings.roughness_cutoff_micro) / 1_000_000.0,
            f32::from(u8::from(self.frame.unlit || !self.ssr.settings.enabled)),
            gpu::scalar(self.color.size()[0]),
            gpu::scalar(self.color.size()[1]),
            if self.ssr.settings.resolution_scale_micro == 500_000 {
                2.0
            } else {
                1.0
            },
            f32::from(u8::from(
                self.ssr.composition
                    == worldrunner_render_lighting::reflections::Composition::ReplaceEnvironment,
            )),
        ]);
        values.extend(gpu::controls(&self.ssr.settings));
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
            PassSlot::ReflectionTrace => (
                if self.water.is_some() {
                    &self.ssr.water_trace
                } else {
                    &self.ssr.trace
                },
                0,
            ),
            PassSlot::ReflectionMip(index) => {
                (&self.ssr.reduce, usize::try_from(index).expect("mip index"))
            }
            PassSlot::ScreenSpaceAfterTransparent => (
                if self.water.is_some() {
                    &self.ssr.water_resolve
                } else {
                    &self.ssr.resolve
                },
                self.ssr.reflections.len(),
            ),
            PassSlot::ReflectionDepthPyramid(level) => {
                let index = usize::try_from(level).expect("depth levels");
                (
                    self.ssr.final_depth.pipeline(index),
                    self.ssr.reflections.len() + 1 + index,
                )
            }
            _ => unreachable!("reflection slot"),
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.groups[index], &[]);
        pass.draw(0..3, 0..1);
    }
}
