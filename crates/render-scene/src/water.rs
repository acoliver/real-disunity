//! Planar water reads immutable opaque snapshots and writes surface depth.
mod endpoint;
mod geometry;
mod optics;
pub(crate) mod planning;
mod program;
pub(crate) mod reactive;
pub(crate) mod receivers;
pub(crate) mod shadow_filter;
#[cfg(test)]
pub(crate) mod shadow_probe;
mod shadow_response;
pub(crate) mod snapshots;
pub(crate) mod surface;
mod terms;
#[cfg(test)]
mod tests;
mod vertex_seeds;

pub(crate) use geometry::allocations;
pub(crate) const STAGING_BYTES: u64 = 560 + crate::frame::FRAME_UNIFORM_BYTES as u64;
pub(crate) const UNIFORM_BYTES: u64 = 768 + crate::frame::FRAME_UNIFORM_BYTES as u64;

use crate::frame::FrameUniform;
use crate::renderer::depth::DepthMode;
use geometry::Body;
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

type RasterKey = ([u32; 5], DepthMode);

#[derive(Clone, Debug)]
pub(crate) struct Water {
    pub draw_count: crate::draw::pass_statistics::PassCounter,
    pub settings: std::sync::Arc<[SceneWaterBody]>,
    bodies: std::sync::Arc<[Body]>,
    pub snapshots: [Snapshots; 3],
    /// The runtime and programs the bodies draw with, shared by every feature made from this
    /// water and by its resized replacements.
    programs: std::rc::Rc<std::cell::RefCell<program::Programs>>,
    layout: wgpu::BindGroupLayout,
    samples: u32,
    depth: DepthMode,
    reactive: reactive::Reactive,
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
        let authority = self.programs.borrow().runtime().authority().clone();
        let mut replacement = Self::new(device, &authority, config, self.settings.to_vec())?;
        replacement.programs = self.programs.clone();
        replacement.environment.clone_from(&self.environment);
        replacement.environment_intensity_micro = self.environment_intensity_micro;
        replacement.surface_inputs.clone_from(&self.surface_inputs);
        Ok(replacement)
    }
    pub fn new(
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
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
        let bodies = geometry
            .into_iter()
            .zip(&settings)
            .map(|(vertices, settings)| {
                Body::new(
                    device,
                    &vertices,
                    endpoint::coordinates(settings).as_deref(),
                )
            })
            .collect::<Vec<_>>()
            .into();
        Ok(Self {
            draw_count: crate::draw::pass_statistics::PassCounter::default(),
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
            programs: std::rc::Rc::new(std::cell::RefCell::new(program::Programs::new(
                device, authority,
            ))),
            samples: config.samples,
            depth: DepthMode::Independent,
            reactive: reactive::Reactive::default(),
            layout,
        })
    }
    #[cfg(test)]
    pub fn feature(
        &self,
        gpu: (&wgpu::Device, &wgpu::CommandEncoder),
        frame: &FrameUniform,
        timing: (f32, usize),
        inputs: (Option<&wgpu::BindGroup>, Option<&wgpu::Texture>),
    ) -> WaterFeature {
        self.feature_with_depth(gpu, frame, timing, inputs, DepthMode::Independent)
            .expect("the test frame belongs to water's authority")
    }

    /// The feature of frame slot `timing.1`, its body draws prepared for passes with `depth`
    /// and retained by `encoder`'s frame.
    ///
    /// # Errors
    /// Fails when a body's program cannot be built or its draw prepared, and with
    /// `ForeignAuthority` when `encoder` is a frame of another authority than the one water was
    /// installed with.
    pub fn feature_with_depth(
        &self,
        (device, encoder): (&wgpu::Device, &wgpu::CommandEncoder),
        frame: &FrameUniform,
        timing: (f32, usize),
        inputs: (Option<&wgpu::BindGroup>, Option<&wgpu::Texture>),
        depth: DepthMode,
    ) -> Result<WaterFeature, worldrunner_render_program::error::ProgramError> {
        let (time, slot) = timing;
        let (shadow, mask) = inputs;
        let snapshots = &self.snapshots[slot];
        let mut water = self.clone();
        water.depth = depth;
        let base = frame.specular_convention.shader_options();
        let options = [
            base[0],
            base[1],
            base[2],
            base[3],
            terms::mask(&self.settings),
        ];
        let draws = water.body_draws((options, shadow.is_some()), slot, encoder)?;
        let mask = water.mask_draw(device, mask, options);
        // The reactive mask still binds the fixed groups.
        let groups = if mask.is_some() {
            self.bodies
                .iter()
                .enumerate()
                .map(|(index, body)| {
                    self.snapshot_group(device, snapshots, &body.uniforms[slot], index)
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(WaterFeature {
            water,
            snapshots: snapshots.clone(),
            frame: *frame,
            time,
            slot,
            draws,
            groups,
            shadow: shadow.cloned(),
            mask,
        })
    }

    /// Draws bodies of `key` with `source` compiled for `options` in place of the composed text.
    #[cfg(test)]
    pub(crate) fn substitute(&self, key: ([u32; 5], bool), source: &str, options: [u32; 5]) {
        self.programs.borrow_mut().substitute(key, source, options);
    }

    /// The prepared draw of each body with vertices, `None` for one without, for frame slot
    /// `slot`. A body draws with the frame's options and its own vertex seed key.
    fn body_draws(
        &self,
        (options, shadows): ([u32; 5], bool),
        slot: usize,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<
        Vec<Option<worldrunner_render_program::runtime::PreparedProgramDraw>>,
        worldrunner_render_program::error::ProgramError,
    > {
        let mut programs = self.programs.borrow_mut();
        let snapshots = &self.snapshots[slot];
        let owner: worldrunner_render_program::runtime::Keepalive =
            std::sync::Arc::new(self.bodies.clone());
        self.bodies
            .iter()
            .zip(self.settings.iter())
            .enumerate()
            .map(|(index, (body, settings))| {
                if body.vertices == 0 {
                    return Ok(None);
                }
                let mut key = options;
                key[4] |= vertex_seeds::key(settings);
                let inputs = program::BodyInputs {
                    uniform: &body.uniforms[slot],
                    snapshot_color: &snapshots.color,
                    snapshot_depth: &snapshots.depth,
                    environment: &self.environment,
                    sampler: &self.sampler,
                    surface: self.surface_inputs[index].each_ref().map(Option::as_ref),
                    surface_sampler: &self.surface_samplers[index],
                    vertices: (&body.buffer, body.vertices),
                    seed: body.seed.as_ref(),
                };
                programs
                    .prepare(
                        ((key, shadows), self.depth, self.samples),
                        &inputs,
                        &owner,
                        encoder,
                    )
                    .map(Some)
            })
            .collect()
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
            include_str!("water/ambient_visibility.wgsl"),
            "\n",
            include_str!("water/optics.wgsl"),
            "\n",
            include_str!("water/water.wgsl"),
            "\n",
            include_str!("water/screen.wgsl"),
            "\n",
            include_str!("water/terms.wgsl"),
            "
",
            include_str!("water/shadow_lookup.wgsl"),
            "
",
            include_str!("water/shadow_response.wgsl"),
            "
",
            include_str!("water/shadow_visibility.wgsl")
        )
    )
}

pub(crate) struct WaterFeature {
    water: Water,
    snapshots: Snapshots,
    frame: FrameUniform,
    time: f32,
    slot: usize,
    /// Each body's draw, prepared and retained by the frame; `None` for a body without vertices.
    draws: Vec<Option<worldrunner_render_program::runtime::PreparedProgramDraw>>,
    /// The fixed groups of the reactive mask, built only when the mask draws.
    groups: Vec<wgpu::BindGroup>,
    shadow: Option<wgpu::BindGroup>,
    mask: Option<reactive::MaskDraw>,
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
        let mask_pass = self.schedule_mask(passes, views)?;
        let mut feature = FeatureDeclaration::new(PassSlot::TransparentWater)
            .passes
            .remove(0);
        feature.samples = self.snapshots.samples;
        feature.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        feature.depth_format = Some(self.water.depth.format());
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
        declaration.passes.extend(mask_pass);
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
                let mut count = crate::draw::DrawCount::default();
                if let Some(shadow) = &self.shadow {
                    pass.set_bind_group(1, shadow, &[]);
                }
                for draw in self.draws.iter().flatten() {
                    draw.record(pass);
                    count.draw(1);
                }
                self.water.draw_count.record(count);
            }
            PassSlot::TransparentWaterMask => self.draw_mask(pass),
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

fn input_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("water inputs"),
        entries: &input_entries(),
    })
}

/// Set 0 of the fixed groups the reactive mask binds. `program::bindings` declares the same set
/// for the body draw; `program_tests` holds the two together.
fn input_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
    [
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
    .collect()
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
