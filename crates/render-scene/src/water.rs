//! Planar water reads immutable opaque snapshots and writes surface depth.
mod geometry;
mod optics;
pub(crate) mod planning;
pub(crate) mod receivers;
pub(crate) mod snapshots;
pub(crate) mod surface;
mod terms;
#[cfg(test)]
mod tests;

pub(crate) const STAGING_BYTES: u64 = 560 + crate::frame::FRAME_UNIFORM_BYTES as u64;
pub(crate) const UNIFORM_BYTES: u64 = 768 + crate::frame::FRAME_UNIFORM_BYTES as u64;

use crate::frame::FrameUniform;
use snapshots::Snapshots;
use worldrunner_render_plan::SceneWaterBody;
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
pub fn validate_settings(settings: &SceneWaterBody) -> Result<(), FrameError> {
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
    pub settings: std::sync::Arc<[SceneWaterBody]>,
    bodies: std::sync::Arc<[Body]>,
    pub snapshots: [Snapshots; 3],
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    specializations: std::rc::Rc<
        std::cell::RefCell<std::collections::BTreeMap<[u32; 5], [wgpu::RenderPipeline; 2]>>,
    >,
    samples: u32,
    pub receivers: [[wgpu::Texture; 3]; 3],
    pub environment: [wgpu::TextureView; 2],
    pub environment_intensity_micro: u32,
    sampler: wgpu::Sampler,
    surface_samplers: Vec<wgpu::Sampler>,
    pub surface_inputs: Vec<[Option<wgpu::TextureView>; 3]>,
}
impl Water {
    pub fn resized(
        &self,
        device: &wgpu::Device,
        config: worldrunner_render_post::FrameConfig,
    ) -> Result<Self, FrameError> {
        let mut replacement = Self::new(device, config, self.settings.to_vec())?;
        replacement.environment.clone_from(&self.environment);
        replacement.environment_intensity_micro = self.environment_intensity_micro;
        replacement.surface_inputs.clone_from(&self.surface_inputs);
        Ok(replacement)
    }
    pub fn new(
        device: &wgpu::Device,
        config: worldrunner_render_post::FrameConfig,
        settings: Vec<SceneWaterBody>,
    ) -> Result<Self, FrameError> {
        if config.format != wgpu::TextureFormat::Rgba16Float
            || !matches!(config.samples, 1 | 2 | 4 | 8)
        {
            return Err(FrameError::WaterSettings(
                "water requires HDR at 1, 2, 4 or 8 samples",
            ));
        }
        let geometry = settings
            .iter()
            .map(geometry::vertices)
            .collect::<Result<Vec<_>, _>>()?;
        if geometry.iter().any(|vertices| {
            u64::try_from(vertices.len()).expect("bounded geometry") * 12
                > device.limits().max_buffer_size
        }) {
            return Err(FrameError::WaterSettings(
                "water geometry exceeds device buffer limit",
            ));
        }
        let layout = input_layout(device);
        let plain_pipeline = pipeline(device, config.samples, &layout, false, [0; 5]);
        let shadow_pipeline = pipeline(device, config.samples, &layout, true, [0; 5]);
        let bodies = geometry
            .into_iter()
            .map(|vertices| Body::new(device, &vertices))
            .collect::<Vec<_>>()
            .into();
        Ok(Self {
            receivers: std::array::from_fn(|_| receivers::textures(device, config)),
            environment: receivers::empty_environment(device),
            environment_intensity_micro: 1_000_000,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            }),
            surface_inputs: vec![[None, None, None]; settings.len()],
            surface_samplers: surface::samplers(device, &settings),
            bodies,
            settings: settings.into(),
            snapshots: std::array::from_fn(|_| Snapshots::new(device, config)),
            specializations: std::rc::Rc::default(),
            samples: config.samples,
            pipeline: plain_pipeline,
            shadow_pipeline,
            layout,
        })
    }
    pub fn feature(
        &self,
        device: &wgpu::Device,
        frame: &FrameUniform,
        timing: (f32, usize),
        shadow: Option<&wgpu::BindGroup>,
    ) -> WaterFeature {
        let (time, slot) = timing;
        let snapshots = &self.snapshots[slot];
        let groups = self
            .bodies
            .iter()
            .enumerate()
            .map(|(index, body)| {
                self.snapshot_group(device, snapshots, &body.uniforms[slot], index)
            })
            .collect();
        let mut water = self.clone();
        let base = frame.specular_convention.shader_options();
        let options = [
            base[0],
            base[1],
            base[2],
            base[3],
            terms::mask(&self.settings),
        ];
        if options != [0; 5] {
            let mut cache = self.specializations.borrow_mut();
            let selected = cache.entry(options).or_insert_with(|| {
                [
                    pipeline(device, self.samples, &self.layout, false, options),
                    pipeline(device, self.samples, &self.layout, true, options),
                ]
            });
            water.pipeline = selected[0].clone();
            water.shadow_pipeline = selected[1].clone();
        }
        WaterFeature {
            water,
            snapshots: snapshots.clone(),
            frame: *frame,
            time,
            slot,
            groups,
            shadow: shadow.cloned(),
        }
    }
}
impl Water {
    fn snapshot_group(
        &self,
        device: &wgpu::Device,
        snapshots: &Snapshots,
        uniform: &wgpu::Buffer,
        index: usize,
    ) -> wgpu::BindGroup {
        let color = snapshots
            .color
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = snapshots
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let uniform_entry = |binding, offset, size| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: uniform,
                offset,
                size: wgpu::BufferSize::new(size),
            }),
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water snapshots"),
            layout: &self.layout,
            entries: &[
                uniform_entry(0, 0, 560),
                surface::binding(1, &color),
                surface::binding(2, &depth),
                uniform_entry(3, 768, crate::frame::FRAME_UNIFORM_BYTES as u64),
                surface::binding(4, &self.environment[0]),
                surface::binding(5, &self.environment[1]),
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                surface::binding(
                    7,
                    self.surface_inputs[index][0]
                        .as_ref()
                        .unwrap_or(&self.environment[1]),
                ),
                surface::binding(
                    10,
                    self.surface_inputs[index][2]
                        .as_ref()
                        .unwrap_or(&self.environment[1]),
                ),
                surface::binding(
                    8,
                    self.surface_inputs[index][1]
                        .as_ref()
                        .unwrap_or(&self.environment[1]),
                ),
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&self.surface_samplers[index]),
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
    options: [u32; 5],
) -> wgpu::RenderPipeline {
    pipeline_with_source(
        device,
        samples,
        layout,
        shadows,
        options,
        &water_source(shadows),
    )
}
fn pipeline_with_source(
    device: &wgpu::Device,
    samples: u32,
    layout: &wgpu::BindGroupLayout,
    shadows: bool,
    options: [u32; 5],
    source: &str,
) -> wgpu::RenderPipeline {
    let constants = terms::constants(options);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("planar water"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let vertex_constants = terms::vertex_constants(options);
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
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &vertex_constants,
                ..Default::default()
            },
            buffers: &[Some(vertex_layout())],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_water"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &constants,
                ..Default::default()
            },
            targets: &std::array::from_fn::<_, 4, _>(|_| {
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })
            }),
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

pub(crate) fn water_source(shadows: bool) -> String {
    let shadow_source = if shadows {
        format!(
            "{}\n{}",
            worldrunner_render_shadow::shadow_wgsl!(),
            include_str!("shader/shadows.wgsl").replace("@group(2)", "@group(1)")
        )
    } else {
        include_str!("shader/unshadowed.wgsl").to_owned()
    };
    format!(
        "{}\n{}\n{}\n{}\n{shadow_source}\n{}",
        worldrunner_render_lighting::lighting_wgsl!(),
        worldrunner_render_post::post_wgsl!(),
        include_str!("shader/lighting.wgsl").replace("@binding(0)", "@binding(3)"),
        include_str!("shader/camera_rays.wgsl"),
        concat!(
            include_str!("water/optics.wgsl"),
            "\n",
            include_str!("water/water.wgsl"),
            "\n",
            include_str!("water/screen.wgsl"),
            "\n",
            include_str!("water/terms.wgsl")
        )
    )
}

pub(crate) struct WaterFeature {
    water: Water,
    snapshots: Snapshots,
    frame: FrameUniform,
    time: f32,
    slot: usize,
    groups: Vec<wgpu::BindGroup>,
    shadow: Option<wgpu::BindGroup>,
}
impl WaterFeature {
    pub fn schedule(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
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
        water.additional_colors.clear();
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
        for view in receivers::views(&self.water.receivers[self.slot])? {
            let mut attachment = crate::compositor::attachment(view.resource(), &view, false);
            attachment.load = AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT);
            water.additional_colors.push(attachment);
            views.push(view);
        }
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
        for (s, body) in self.water.settings.iter().zip(self.water.bodies.iter()) {
            let mut data = uniform_data(&self.frame, self.time, s);
            // The ambient vector has a spare lane for the independent cube scale.
            data[75] = number(i64::from(self.water.environment_intensity_micro)) / 1000.0;
            context.upload_buffer(&body.uniforms[self.slot], 768, &self.frame.to_bytes());
            context.upload_buffer(
                &body.uniforms[self.slot],
                0,
                &data
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
        }
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

                if let Some(shadow) = &self.shadow {
                    pass.set_bind_group(1, shadow, &[]);
                }
                for (body, group) in self.water.bodies.iter().zip(&self.groups) {
                    if body.vertices != 0 {
                        pass.set_bind_group(0, group, &[]);
                        pass.set_vertex_buffer(0, body.buffer.slice(..));
                        pass.draw(0..body.vertices, 0..1);
                    }
                }
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

#[derive(Clone, Debug)]
struct Body {
    buffer: wgpu::Buffer,
    vertices: u32,
    uniforms: [wgpu::Buffer; 3],
}
impl Body {
    fn new(device: &wgpu::Device, vertices: &[[f32; 3]]) -> Self {
        use wgpu::util::DeviceExt;
        let bytes: Vec<_> = vertices
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        Self {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water body surface"),
                contents: if bytes.is_empty() { &[0; 12] } else { &bytes },
                usage: wgpu::BufferUsages::VERTEX,
            }),
            vertices: u32::try_from(vertices.len()).expect("bounded water geometry"),
            uniforms: std::array::from_fn(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("water body frame"),
                    size: UNIFORM_BYTES,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            }),
        }
    }
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    wgpu::VertexBufferLayout {
        array_stride: 12,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn input_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        ]
        .into_iter()
        .chain(receivers::environment_entries())
        .chain(surface::entries())
        .collect::<Vec<_>>(),
    })
}

fn uniform_data(frame: &FrameUniform, time: f32, s: &SceneWaterBody) -> Vec<f32> {
    let mut data = Vec::new();
    for matrix in [
        frame.view_projection,
        frame.view,
        frame.view_projection.inverse(),
    ] {
        data.extend(matrix.to_cols_array());
    }
    data.extend([
        number(s.east_min_millimeters),
        number(s.north_min_millimeters),
        number(s.height_millimeters),
        time,
    ]);
    data.extend([
        number(i64::try_from(s.east_extent_millimeters).expect("validated extent")),
        number(i64::try_from(s.north_extent_millimeters).expect("validated extent")),
        number(i64::from(s.flow_millimeters_per_second[0])),
        number(i64::from(s.flow_millimeters_per_second[1])),
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
    data.push(number(i64::from(
        s.material.flow_period_milliseconds.unwrap_or(0),
    )));
    data.extend(frame.sun_direction.to_array());
    data.push(0.0);
    data.extend(frame.sun_irradiance.to_array());
    data.push(0.0);
    data.extend(frame.ambient_sky.to_array());
    data.push(0.0);
    data.extend([
        1.0 / number(i64::from(s.material.tile_millimeters[0])),
        1.0 / number(i64::from(s.material.tile_millimeters[1])),
        f32::from(u8::from(s.material.normal.is_some())),
        number(i64::from(s.material.flow_speed_millimeters_per_second))
            * f32::from(u8::from(s.material.flow.is_some())),
    ]);
    data.extend(optics::uniforms(s));
    data.extend(terms::uniforms(s, frame.animation_time_milliseconds));
    data
}
