//! Planar water reads immutable opaque snapshots and writes surface depth.
pub(crate) mod planning;
pub(crate) mod snapshots;
#[cfg(test)]
mod tests;

use crate::frame::FrameUniform;
use snapshots::Snapshots;
use worldrunner_render_plan::SceneWaterPlane;
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FeatureDeclaration, FrameError, PassDeclaration, PassSlot,
    PreparedFeature, ResourceRef,
};

#[derive(Default)]
pub(crate) struct WaterState {
    pub feature: Option<WaterFeature>,
}

pub(crate) struct WaterRegistration {
    pub state: std::rc::Rc<std::cell::RefCell<WaterState>>,
    pub samples: u32,
}
impl worldrunner_render_post::FrameFeature for WaterRegistration {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::TransparentWater);
        declaration.passes[0].samples = self.samples;
        declaration.passes[0].color_format = Some(wgpu::TextureFormat::Rgba16Float);
        declaration.passes[0].depth_format = Some(wgpu::TextureFormat::Depth32Float);
        declaration.passes[0].reads = vec![
            ResourceRef::new("opaque snapshot color"),
            ResourceRef::new("opaque snapshot depth"),
        ];
        declaration.effects.push("water");
        declaration
    }
    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        self.state
            .borrow()
            .feature
            .as_ref()
            .ok_or(FrameError::WaterSettings("missing frame input"))?
            .schedule(passes, views)
    }
    fn build(
        &self,
        _: &worldrunner_render_post::BuildContext<'_>,
    ) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(WaterPrepared(self.state.clone())))
    }
}
struct WaterPrepared(std::rc::Rc<std::cell::RefCell<WaterState>>);
impl PreparedFeature for WaterPrepared {
    fn prepare(
        &mut self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        self.0
            .borrow_mut()
            .feature
            .as_mut()
            .expect("validated water input")
            .prepare(context)
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.0
            .borrow_mut()
            .feature
            .as_mut()
            .expect("validated water input")
            .commit(context);
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        self.0
            .borrow()
            .feature
            .as_ref()
            .expect("validated water input")
            .draw(context, pass);
    }
}

/// Validates the bounded stock water model before allocating GPU resources.
pub fn validate_settings(settings: &SceneWaterPlane) -> Result<(), FrameError> {
    if settings.roughness_micro > 1_000_000 {
        return Err(FrameError::WaterSettings("roughness must be 0..=1000000"));
    }
    if settings.color_micro.iter().any(|value| *value > 1_000_000)
        || settings
            .absorption_micro
            .iter()
            .any(|value| *value > 10_000_000)
    {
        return Err(FrameError::WaterSettings(
            "colour or absorption exceeds the stock range",
        ));
    }
    if settings.validate().is_err() {
        return Err(FrameError::WaterSettings("invalid rectangle"));
    }
    Ok(())
}

/// Beer transmittance for a nonnegative distance through the medium, in metres.
pub fn transmittance(absorption_per_meter: f32, distance_meters: f32) -> f32 {
    (-absorption_per_meter * distance_meters.max(0.0)).exp()
}

/// Foreground samples cannot supply a refracted background behind the surface.
pub fn refraction_pixel(
    original: [i32; 2],
    distorted: [i32; 2],
    sampled_depth: f32,
    surface_depth: f32,
) -> [i32; 2] {
    if sampled_depth < surface_depth {
        original
    } else {
        distorted
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Water {
    pub settings: SceneWaterPlane,
    pub snapshots: [Snapshots; 3],
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}
impl Water {
    pub fn new(
        device: &wgpu::Device,
        config: worldrunner_render_post::FrameConfig,
        settings: SceneWaterPlane,
    ) -> Result<Self, FrameError> {
        validate_settings(&settings)?;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water inputs"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1),
                texture_entry(2),
            ],
        });
        let plain_pipeline = pipeline(device, config.samples, &layout, false);
        let shadow_pipeline = pipeline(device, config.samples, &layout, true);
        Ok(Self {
            settings,
            snapshots: std::array::from_fn(|_| Snapshots::new(device, config)),
            pipeline: plain_pipeline,
            shadow_pipeline,
            layout,
        })
    }
    pub fn feature(
        &self,
        device: &wgpu::Device,
        frame: FrameUniform,
        timing: (f32, usize),
        shadow: Option<&wgpu::BindGroup>,
    ) -> WaterFeature {
        let (time, slot) = timing;
        let snapshots = &self.snapshots[slot];
        let group = self.snapshot_group(device, snapshots);
        WaterFeature {
            water: self.clone(),
            snapshots: snapshots.clone(),
            frame,
            time,
            group,
            shadow: shadow.cloned(),
        }
    }
}
impl Water {
    fn snapshot_group(&self, device: &wgpu::Device, snapshots: &Snapshots) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water snapshots"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: snapshots.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &snapshots
                            .color
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &snapshots
                            .depth
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
            ],
        })
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
fn pipeline(
    device: &wgpu::Device,
    samples: u32,
    layout: &wgpu::BindGroupLayout,
    shadows: bool,
) -> wgpu::RenderPipeline {
    let shadow_source = if shadows {
        format!(
            "{}\n{}",
            worldrunner_render_shadow::shadow_wgsl!(),
            include_str!("shader/shadows.wgsl").replace("@group(2)", "@group(1)")
        )
    } else {
        "fn sunlight_visibility(p: vec3<f32>, n: vec3<f32>) -> f32 { return 1.0; }".to_owned()
    };
    let source = format!("{shadow_source}\n{}", include_str!("water/water.wgsl"));
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("planar water"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let shadow_layout = crate::shadows::water_layout(device);
    let layouts = if shadows {
        vec![Some(layout), Some(&shadow_layout)]
    } else {
        vec![Some(layout)]
    };
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("water"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("water"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_water"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_water"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

pub(crate) struct WaterFeature {
    water: Water,
    snapshots: Snapshots,
    frame: FrameUniform,
    time: f32,
    group: wgpu::BindGroup,
    shadow: Option<wgpu::BindGroup>,
}
impl WaterFeature {
    pub fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &[AttachmentView],
    ) -> Result<FeatureDeclaration, FrameError> {
        let config = worldrunner_render_post::FrameConfig::new(
            self.snapshots.size,
            wgpu::TextureFormat::Rgba16Float,
            self.snapshots.samples,
        );
        planning::opaque_source(passes, views, config, false)?;
        planning::opaque_source(passes, views, config, true)?;
        let mut declaration = FeatureDeclaration::new(PassSlot::TransparentWater);
        declaration.passes.clear();
        let mut water = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::Opaque)
            .ok_or(FrameError::MissingSlot(PassSlot::Opaque))?
            .clone();
        water.slot = PassSlot::TransparentWater;
        water
            .color
            .as_mut()
            .ok_or(FrameError::AttachmentContract(PassSlot::Opaque))?
            .load = AttachmentLoad::Load;
        water
            .depth
            .as_mut()
            .ok_or(FrameError::AttachmentContract(PassSlot::Opaque))?
            .load = AttachmentLoad::Load;
        passes.push(water);
        let mut feature = FeatureDeclaration::new(PassSlot::TransparentWater)
            .passes
            .remove(0);
        feature.samples = self.snapshots.samples;
        feature.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        feature.depth_format = Some(wgpu::TextureFormat::Depth32Float);
        feature.reads = vec![
            ResourceRef::new("opaque snapshot color"),
            ResourceRef::new("opaque snapshot depth"),
        ];
        if self.shadow.is_some() {
            feature.reads.extend(
                views
                    .iter()
                    .map(AttachmentView::resource)
                    .filter(|resource| resource.name == "shadow depth"),
            );
        }
        declaration.passes.push(feature);
        declaration.effects.push("water");
        Ok(declaration)
    }
}
impl PreparedFeature for WaterFeature {
    fn prepare(
        &mut self,
        _: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        let s = self.water.settings;
        let mut data = Vec::new();
        for matrix in [
            self.frame.view_projection,
            self.frame.view,
            self.frame.view_projection.inverse(),
        ] {
            data.extend(matrix.to_cols_array());
        }
        data.extend([
            number(s.east_min_millimeters),
            number(s.north_min_millimeters),
            number(s.height_millimeters),
            self.time,
        ]);
        data.extend([
            number(i64::try_from(s.east_extent_millimeters).expect("validated extent")),
            number(i64::try_from(s.north_extent_millimeters).expect("validated extent")),
            0.0,
            0.0,
        ]);
        data.extend(
            s.color_micro
                .map(|v| crate::view::meters_scalar(v) / 1000.0),
        );
        data.push(crate::view::meters_scalar(s.roughness_micro) / 1000.0);
        data.extend(
            s.absorption_micro
                .map(|v| crate::view::meters_scalar(v) / 1000.0),
        );
        data.push(0.0);
        data.extend(self.frame.sun_direction.to_array());
        data.push(0.0);
        data.extend(self.frame.sun_irradiance.to_array());
        data.push(0.0);
        data.extend(self.frame.ambient_sky.to_array());
        data.push(0.0);
        context.upload_buffer(
            &self.snapshots.uniform,
            0,
            &data
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
        match context.slot {
            PassSlot::TransparentWater => {
                pass.set_pipeline(if self.shadow.is_some() {
                    &self.water.shadow_pipeline
                } else {
                    &self.water.pipeline
                });
                pass.set_bind_group(0, &self.group, &[]);
                if let Some(shadow) = &self.shadow {
                    pass.set_bind_group(1, shadow, &[]);
                }
                pass.draw(0..6, 0..1);
            }
            _ => unreachable!("declared water slot"),
        }
    }
}
fn number(value: i64) -> f32 {
    // Decimal parsing avoids an unchecked integer narrowing at the scene boundary.
    value
        .to_string()
        .parse::<f32>()
        .expect("integer is a finite float")
        / 1000.0
}
