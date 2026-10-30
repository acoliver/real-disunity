//! Opt-in atmospheric background in the existing opaque scene pass.
//! All directions are scene Z-up; the shared frame owns exposure and solar direction.
use glam::Vec3;
use std::{cell::RefCell, collections::BTreeMap};
use wgpu::util::DeviceExt;
use worldrunner_render_plan::{SceneAtmosphere, SceneCelestialDisc};

#[cfg(test)]
mod body_lighting_tests;
#[cfg(test)]
mod controls_tests;
pub(crate) mod lighting;
mod mesh_target;
pub(crate) mod meshes;
pub mod preparation;
mod scattering;
#[cfg(test)]
mod ssr_tests;
const SOURCE: &str = concat!(
    worldrunner_render_lighting::lighting_wgsl!(),
    include_str!("medium.wgsl"),
    include_str!("sky.wgsl"),
    include_str!("../shader/camera_rays.wgsl")
);

/// Angular disc mapping shared by descriptor tests and tools. The radius is in radians.
pub fn disc_uv(ray: Vec3, direction: Vec3, radius: f32, rotation: f32) -> Option<[f32; 2]> {
    let cosine = ray.dot(direction);
    if cosine < radius.cos() {
        return None;
    }
    let seed = if direction.z.abs() < 0.999 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let right = seed.cross(direction).normalize();
    let up = direction.cross(right);
    let x = ray.dot(right) / (cosine * radius.tan());
    let y = ray.dot(up) / (cosine * radius.tan());
    let (s, c) = rotation.sin_cos();
    Some([0.5 + (c * x + s * y) * 0.5, 0.5 + (c * y - s * x) * 0.5])
}

pub(crate) fn depth_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: crate::renderer::DEPTH_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::LessEqual),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}
fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("atmospheric background"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(160),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}
pub(crate) fn pipeline(
    device: &wgpu::Device,
    frame: &wgpu::BindGroupLayout,
    target: (wgpu::TextureFormat, u32, bool),
    reflection: bool,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let group = layout(device);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sky scene pipeline"),
        bind_group_layouts: &[Some(frame), Some(&group)],
        immediate_size: 0,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sky"),
        source: wgpu::ShaderSource::Wgsl(SOURCE.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky background"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth_state()),
        multisample: wgpu::MultisampleState {
            count: target.1,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets(
                wgpu::ColorTargetState {
                    format: target.0,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                },
                (target.2, reflection),
            ),
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, group)
}

#[derive(Debug)]
pub(crate) struct Sky {
    variants: RefCell<BTreeMap<(bool, bool), wgpu::RenderPipeline>>,
    device: wgpu::Device,
    frame_layout: wgpu::BindGroupLayout,
    target: (wgpu::TextureFormat, u32, bool),
    groups: Vec<wgpu::BindGroup>,
    lookup: Option<scattering::Lookup>,
    identity: std::rc::Rc<()>,
}
impl Sky {
    pub fn upload(
        device: &wgpu::Device,
        scene: &worldrunner_render_plan::RenderScene,
        pipelines: &crate::renderer::ScenePipelines,
        views: &[wgpu::TextureView],
        white: &wgpu::TextureView,
    ) -> Result<Option<Self>, preparation::PreparationError> {
        scene
            .effects()
            .and_then(|effects| effects.atmosphere())
            .map(|settings| {
                if let Some(physical) = settings.scattering {
                    preparation::atmosphere_allocations(physical)?
                        .validate(&device.limits())
                        .map_err(|_| preparation::PreparationError::Budget)?;
                }
                Ok(Self::new(
                    device,
                    pipelines.frame_layout(),
                    (
                        pipelines.color_format,
                        pipelines.samples,
                        pipelines.writes_velocity(),
                    ),
                    settings,
                    match scene.effects().expect("atmosphere has effects").sky() {
                        worldrunner_render_plan::SceneSky::DomeTexture { texture } => {
                            Some(*texture)
                        }
                        worldrunner_render_plan::SceneSky::ProceduralHemisphere { .. } => None,
                    },
                    (views, white),
                ))
            })
            .transpose()
    }

    pub fn new(
        device: &wgpu::Device,
        frame: &wgpu::BindGroupLayout,
        target: (wgpu::TextureFormat, u32, bool),
        settings: &SceneAtmosphere,
        dome: Option<worldrunner_render_plan::TextureHandle>,
        textures: (&[wgpu::TextureView], &wgpu::TextureView),
    ) -> Self {
        settings.validate().expect("validated atmosphere");
        let (views, white) = textures;
        let (pipeline, layout) = pipeline(device, frame, target, false);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("celestial sampling"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        let lookup = settings
            .scattering
            .map(|s| scattering::Lookup::new(device, s));
        let columns = scattering::columns(device, lookup.as_ref());
        let groups = std::iter::once(None)
            .chain(settings.discs.iter().map(Some))
            .map(|disc| {
                let bytes = uniform(settings, disc, disc.is_none() && dome.is_some());
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("sky parameters"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let texture = disc.map_or(dome, |disc| disc.texture);
                let view = texture.map_or(white, |h| &views[h.raw() as usize]);
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sky parameters"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&columns),
                        },
                    ],
                })
            })
            .collect();
        Self {
            variants: RefCell::new(BTreeMap::from([((target.2, false), pipeline)])),
            device: device.clone(),
            frame_layout: frame.clone(),
            target,
            groups,
            lookup,
            identity: std::rc::Rc::new(()),
        }
    }
    pub fn prepare(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame: &crate::frame::FrameUniform,
    ) -> Result<(), preparation::PreparationError> {
        if let Some(lookup) = &self.lookup {
            crate::compositor::publication::encode(encoder, &self.identity, || {})?;
            lookup.encode(
                device,
                encoder,
                (frame.sun_direction, frame.camera_position.z.max(0.1)),
                worldrunner_render_gpu::configuration::AllocationBudget(16 * 1024 * 1024),
            )?;
        }
        Ok(())
    }
    pub fn write_key(&self) -> Option<worldrunner_render_gpu::submission::capture::WriteKey> {
        self.lookup
            .as_ref()
            .map(|_| crate::compositor::publication::key(&self.identity))
    }
    pub fn requires_preparation(&self) -> bool {
        self.lookup.is_some()
    }
    pub fn bind_background(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(1, &self.groups[0], &[]);
    }
    #[cfg(test)]
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        self.draw_targets(pass, frame, (self.target.2, false));
    }
    pub fn draw_targets(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &wgpu::BindGroup,
        key: (bool, bool),
    ) {
        let mut variants = self.variants.borrow_mut();
        let pipeline = variants.entry(key).or_insert_with(|| {
            pipeline(
                &self.device,
                &self.frame_layout,
                (self.target.0, self.target.1, key.0),
                key.1,
            )
            .0
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, frame, &[]);
        for group in &self.groups {
            pass.set_bind_group(1, group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
#[expect(
    clippy::cast_precision_loss,
    reason = "bounded scene micro units and angles"
)]
fn uniform(settings: &SceneAtmosphere, disc: Option<&SceneCelestialDisc>, dome: bool) -> Vec<u8> {
    let color = |rgb: [u32; 3], w: f32| {
        [
            rgb[0] as f32 / 1e6,
            rgb[1] as f32 / 1e6,
            rgb[2] as f32 / 1e6,
            w,
        ]
    };
    let mut lanes = vec![
        color(
            settings.horizon_radiance_micro,
            settings.gradient_exponent_micro as f32 / 1e6,
        ),
        color(
            settings.zenith_radiance_micro,
            settings.sun_angular_radius_millidegrees as f32 / 1000.0 * std::f32::consts::PI / 180.0,
        ),
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [
            0.0,
            f32::from(u8::from(dome)),
            0.0,
            settings.background_distance_millimeters as f32 / 1000.0,
        ],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
    ];
    scattering::parameters(settings.scattering, &mut lanes);
    lanes[2][0] = f32::from(u8::from(
        settings.background_depth_policy
            == worldrunner_render_plan::SceneBackgroundDepthPolicy::FarPlane,
    ));
    celestial_parameters(settings, &mut lanes);
    if let Some(disc) = disc {
        let direction =
            Vec3::from_array(disc.direction.components_micro().map(|v| v as f32 / 1e6)).normalize();
        lanes[4] = [
            direction.x,
            direction.y,
            direction.z,
            (disc.angular_radius_millidegrees as f32 / 1000.0).to_radians(),
        ];
        lanes[5] = color(
            disc.radiance_micro,
            (disc.rotation_millidegrees as f32 / 1000.0).to_radians(),
        );
        lanes[6][0] = 1.0;
    }
    lanes
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod render_tests;

#[cfg(test)]
mod scene_tests;

fn celestial_parameters(settings: &SceneAtmosphere, lanes: &mut [[f32; 4]]) {
    lanes[2][1] = match settings.celestial_composition {
        worldrunner_render_plan::SceneCelestialComposition::Inherit => 0.0,
        worldrunner_render_plan::SceneCelestialComposition::SeparateSurface => 1.0,
    };
    if let Some(exposure) = settings.celestial_exposure_micro {
        lanes[2][2] = 1.0;
        lanes[2][3] = (f32::from(u16::try_from(exposure >> 16).expect("upper half")) * 65536.0
            + f32::from(u16::try_from(exposure & 65535).expect("lower half")))
            / 1e6;
    }
}
#[expect(
    clippy::cast_precision_loss,
    reason = "scene integer atmospheric units"
)]
pub(crate) fn height_fog(
    fog: worldrunner_render_plan::SceneHeightFog,
) -> worldrunner_render_lighting::atmosphere::HeightFog {
    let mut result = worldrunner_render_lighting::atmosphere::HeightFog::new(
        fog.density.per_meter(),
        fog.falloff_micro_per_meter as f32 / 1e6,
        fog.base_height_millimeters as f32 / 1000.0,
        fog.start_distance_millimeters as f32 / 1000.0,
        Vec3::from_array(fog.radiance_micro.map(|v| v as f32 / 1e6)),
    )
    .expect("integer atmospheric descriptor is finite and nonnegative");
    result.full_ray_density =
        fog.density_policy == worldrunner_render_plan::SceneFogDensityPolicy::FullRay;
    result
}

/// Resident atmospheric storage to include in a host's scene allocation plan.
/// Referenced scene textures and meshes are already counted by their resource owner.
pub fn allocations(
    settings: &SceneAtmosphere,
    size: [u32; 2],
    samples: u32,
) -> worldrunner_render_gpu::configuration::AllocationSet {
    let mut set = worldrunner_render_gpu::configuration::AllocationSet::default();
    set.buffer(
        "sky parameters",
        160,
        u32::try_from(settings.discs.len() + 1).expect("validated celestial count"),
    );
    if !settings.meshes.is_empty() {
        for (name, format) in [
            ("celestial radiance", wgpu::TextureFormat::Rgba16Float),
            ("celestial depth", wgpu::TextureFormat::Depth32Float),
        ] {
            set.texture(name, [size[0], size[1], 1], format, [1, samples], 1);
        }
        set.buffer(
            "celestial instances",
            crate::geometry::INSTANCE_STRIDE,
            u32::try_from(settings.meshes.len()).expect("validated celestial count"),
        );
    }
    if let Some(physical) = settings.scattering {
        set.extend(scattering::allocations(physical));
    } else {
        set.texture(
            "unused atmospheric columns",
            [1, 1, 3],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
    }
    set
}

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod visibility_tests;

#[cfg(test)]
mod policy_tests;

#[cfg(test)]
mod review_tests;

fn targets(
    colour: wgpu::ColorTargetState,
    key: (bool, bool),
) -> Vec<Option<wgpu::ColorTargetState>> {
    let mut targets = vec![Some(colour)];
    if key.1 {
        targets.extend((0..2).map(|_| {
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })
        }));
    }
    if key.0 {
        targets.push(Some(wgpu::ColorTargetState {
            format: worldrunner_render_post::VELOCITY_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::empty(),
        }));
    }
    targets
}
