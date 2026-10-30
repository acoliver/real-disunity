//! Scheduled opaque reflections with conservative final-scene depth traversal.
use crate::frame::metadata::FrameProduct;
mod final_depth;
pub(crate) mod forward;
mod gpu;
mod grid;
mod metadata;
pub(crate) mod pyramid;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod water;
use crate::{frame::FrameUniform, scene_metadata::SceneMetadata};
use worldrunner_render_plan::{SceneSsao, SceneSsr};
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FeatureDeclaration, FeaturePass, FrameConfig, FrameError,
    PassDeclaration, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};

pub(crate) fn depth_level_count(settings: &SceneSsr, size: [u32; 2]) -> usize {
    if settings.traversal == worldrunner_render_plan::SceneSsrTraversal::Grid {
        let (base, count) = grid::domain(settings, size);
        (base + count).max(7)
    } else {
        7
    }
}
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
    let [width, _] = settings.radiance_extents(config.size)[0];
    if width
        .checked_mul(config.samples)
        .is_none_or(|width| width > limits.max_texture_dimension_2d)
        || config.size[1] > limits.max_texture_dimension_2d
    {
        return Err(FrameError::SsrSettings(
            "sample-packed radiance extent exceeds device texture limit",
        ));
    }
    pyramid::validate(config, settings, limits)?;
    grid::validate(config.size, settings, limits)?;
    Ok(())
}
pub(crate) const fn neutral_ao() -> SceneSsao {
    SceneSsao {
        depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint {
            level_count: 8,
            rounding: worldrunner_render_plan::SceneDownsampleRounding::Ceil,
        },
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
    pub environment_intensity_micro: u32,
    pub composition: worldrunner_render_lighting::reflections::Composition,
    velocity: bool,
    material: wgpu::Texture,
    application_ao: wgpu::Texture,
    material_depth: wgpu::Texture,
    source_depth: wgpu::Texture,
    normals: wgpu::Texture,
    hierarchy: Vec<wgpu::Texture>,
    final_depth: final_depth::FinalDepth,
    reflections: Vec<wgpu::Texture>,
    reduction_extents: Vec<wgpu::Buffer>,
    capture_size: [u32; 2],
    capture_samples: u32,
    uniform: wgpu::Buffer,
    trace: wgpu::RenderPipeline,
    reduce: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
    water_trace: wgpu::RenderPipeline,
    water_resolve: wgpu::RenderPipeline,
}
pub(crate) fn composition(
    selected: worldrunner_render_plan::SceneSsrComposition,
) -> worldrunner_render_lighting::reflections::Composition {
    match selected {
        worldrunner_render_plan::SceneSsrComposition::Additive => {
            worldrunner_render_lighting::reflections::Composition::Additive
        }
        worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment => {
            worldrunner_render_lighting::reflections::Composition::ReplaceEnvironment
        }
    }
}
impl Ssr {
    pub(crate) fn material_target(&self) -> &wgpu::Texture {
        &self.material
    }

    pub(crate) fn confidence_target(&self) -> &wgpu::Texture {
        &self.reflections[0]
    }

    pub(crate) fn confidence_capture(&self) -> crate::compositor::FeatureCapture<'_> {
        let [w, h] = self.capture_size;
        crate::compositor::FeatureCapture {
            texture: self.confidence_target(),
            size: [w * self.capture_samples, h],
            lanes: self.capture_samples,
            columns: if pyramid::shared(&self.settings) {
                pyramid::lane_grid(self.capture_samples)[0]
            } else {
                self.capture_samples
            },
            stride: [w, h],
        }
    }

    pub(crate) fn textures(&self) -> Vec<&wgpu::Texture> {
        let mut textures = vec![&self.material, &self.material_depth];
        textures.extend(self.forward_products.iter());
        let count = if pyramid::shared(&self.settings) {
            1
        } else {
            self.reflections.len()
        };
        textures.extend(self.reflections.iter().take(count));
        textures.extend(self.final_depth.textures());
        textures
    }

    fn resource(&self, index: u32) -> ResourceRef {
        ResourceRef {
            mip: if pyramid::shared(&self.settings) {
                index
            } else {
                0
            },
            ..resource(index)
        }
    }
    fn reflection_view(&self, index: usize) -> wgpu::TextureView {
        pyramid::view(
            &self.reflections[index],
            self.resource(u32::try_from(index).unwrap()).mip,
        )
    }
    fn reflection_views(&self) -> Vec<wgpu::TextureView> {
        if pyramid::shared(&self.settings) {
            vec![self.reflections[0].create_view(&wgpu::TextureViewDescriptor::default())]
        } else {
            (0..self.reflections.len())
                .map(|index| self.reflection_view(index))
                .collect()
        }
    }
    fn application_views(&self, views: &mut [wgpu::TextureView]) {
        if self.settings.depth_relative_ao {
            views[1] = self
                .application_ao
                .create_view(&wgpu::TextureViewDescriptor::default());
        }
    }
    #[cfg(test)]
    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        settings: SceneSsr,
        screen: &mut SceneMetadata,
    ) -> Result<Self, FrameError> {
        Self::with_velocity(device, config, settings, screen, false)
    }

    /// Like [`Self::new`], with forward draws that also write the TAA residual when `velocity`
    /// is set and the device colour budget allows it (see [`crate::velocity::reflection_velocity`]).
    pub fn with_velocity(
        device: &wgpu::Device,
        config: FrameConfig,
        settings: SceneSsr,
        screen: &mut SceneMetadata,
        velocity: bool,
    ) -> Result<Self, FrameError> {
        validate_settings(&settings)?;
        validate_extent(config, &settings, &device.limits())?;
        let velocity = crate::velocity::reflection_writes_velocity(device, config.format, velocity);
        let environment = forward::install(
            device,
            config,
            screen,
            (settings.depth_relative_ao, velocity),
        );
        screen.geometry.reflection =
            Some(crate::scene_metadata::pipelines(device, &screen.uniform, true).0);
        // All geometry pipelines use the same explicit group layout as the normal prepass.

        let material = metadata::material_texture(device, config.size, screen)?;
        let final_depth =
            final_depth::FinalDepth::new(device, config.size, config.samples, &settings);
        let hierarchy = final_depth.trace_levels();
        let reflections = gpu::radiance_textures(device, config, &settings);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSR camera"),
            size: 208,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let trace = grid::pipeline(device, config, &settings, false);
        let reduce = gpu::reduction_pipeline(device, config.samples, &settings);
        let (resolve, water_resolve) = pyramid::resolve_pipelines(
            device,
            config,
            &settings,
            screen.passes.filtered.format() == wgpu::TextureFormat::Rg16Float,
        );
        let water_trace = grid::pipeline(device, config, &settings, true);
        Ok(Self {
            settings,
            environment,
            environment_intensity_micro: 1_000_000,
            forward_products: forward::products(device, config),
            velocity,
            composition: composition(settings.composition),
            water_trace,
            water_resolve,
            material_depth: crate::ssao::passes::texture(
                device,
                config.size,
                wgpu::TextureFormat::Depth32Float,
            ),
            material,
            application_ao: screen.passes.filtered.clone(),
            source_depth: screen.passes.depth_levels()[0].clone(),
            normals: screen.normals.clone(),
            hierarchy,
            final_depth,
            reflections,
            reduction_extents: pyramid::uniforms(device, config, &settings),
            capture_size: settings.radiance_extents(config.size)[0],
            capture_samples: config.samples,
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
        let mut replacement =
            Self::with_velocity(device, config, self.settings, screen, self.velocity)?;
        replacement.environment.clone_from(&self.environment);
        replacement.environment_intensity_micro = self.environment_intensity_micro;
        replacement.composition = self.composition;
        let group = forward::group(
            device,
            screen,
            &replacement.environment,
            replacement.environment_intensity_micro,
        );
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
        for index in 0..ssr.reflections.len() - 1 {
            self.groups.push(gpu::group(
                context.device(),
                &ssr.reduce,
                ssr.reduction_extents.get(index),
                &[ssr.reflection_view(index)],
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
    fn application_reads(&self, feature: &mut FeaturePass) {
        if self.ssr.settings.depth_relative_ao {
            feature.reads.push(ResourceRef::new("ambient occlusion"));
        }
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
            let view = AttachmentView::new(ssr.resource(index), texture)?;
            let mut pass = PassDeclaration::new(slot);
            pass.color = Some(crate::compositor::attachment(view.resource(), &view, false));
            passes.push(pass);
            views.push(view);
            let mut feature = FeaturePass::new(slot);
            feature.color_format = Some(wgpu::TextureFormat::Rgba16Float);
            feature.reads = if index == 0 {
                self.trace_reads()
            } else {
                vec![ssr.resource(index - 1)]
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
            .map(|index| ssr.resource(index))
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
        self.application_reads(&mut feature);
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
        resolve_inputs.extend(ssr.reflection_views());
        resolve_inputs.extend(
            ssr.forward_products
                .each_ref()
                .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
        );
        if let Some(water) = &self.water {
            water.resolve_views(&mut resolve_inputs);
        }
        ssr.application_views(&mut resolve_inputs);
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
            gpu::roughness_cutoff(&self.ssr.settings),
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
        if pyramid::shared(&self.ssr.settings)
            && matches!(
                context.slot,
                PassSlot::ReflectionTrace | PassSlot::ReflectionMip(_)
            )
        {
            let size = self.ssr.settings.radiance_extents(self.color.size())[index];
            let grid = pyramid::lane_grid(self.color.texture().sample_count());
            pass.set_scissor_rect(0, 0, size[0] * grid[0], size[1] * grid[1]);
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.groups[index], &[]);
        pass.draw(0..3, 0..1);
    }
}
