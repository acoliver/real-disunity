//! Opt-in atmospheric background in the existing opaque scene pass.
//! All directions are scene Z-up; the shared frame owns exposure and solar direction.
use glam::Vec3;
use wgpu::util::DeviceExt;
use worldrunner_render_plan::{SceneAtmosphere, SceneCelestialDisc};

mod mesh_target;
pub(crate) mod meshes;
pub mod preparation;
const SOURCE: &str = concat!(
    worldrunner_render_lighting::lighting_wgsl!(),
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
        ],
    })
}
pub(crate) fn pipeline(
    device: &wgpu::Device,
    frame: &wgpu::BindGroupLayout,
    target: (wgpu::TextureFormat, u32),
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
            targets: &[Some(wgpu::ColorTargetState {
                format: target.0,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipeline, group)
}

#[derive(Debug)]
pub(crate) struct Sky {
    pipeline: wgpu::RenderPipeline,
    groups: Vec<wgpu::BindGroup>,
}
impl Sky {
    pub fn upload(
        device: &wgpu::Device,
        scene: &worldrunner_render_plan::RenderScene,
        pipelines: &crate::renderer::ScenePipelines,
        views: &[wgpu::TextureView],
        white: &wgpu::TextureView,
    ) -> Option<Self> {
        scene
            .effects()
            .and_then(|effects| effects.atmosphere())
            .map(|settings| {
                Self::new(
                    device,
                    pipelines.frame_layout(),
                    (pipelines.color_format, pipelines.samples),
                    settings,
                    match scene.effects().expect("atmosphere has effects").sky() {
                        worldrunner_render_plan::SceneSky::DomeTexture { texture } => {
                            Some(*texture)
                        }
                        worldrunner_render_plan::SceneSky::ProceduralHemisphere { .. } => None,
                    },
                    (views, white),
                )
            })
    }

    pub fn new(
        device: &wgpu::Device,
        frame: &wgpu::BindGroupLayout,
        target: (wgpu::TextureFormat, u32),
        settings: &SceneAtmosphere,
        dome: Option<worldrunner_render_plan::TextureHandle>,
        textures: (&[wgpu::TextureView], &wgpu::TextureView),
    ) -> Self {
        let (views, white) = textures;
        let (pipeline, layout) = pipeline(device, frame, target);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("celestial sampling"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
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
                    ],
                })
            })
            .collect();
        Self { pipeline, groups }
    }
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, frame: &wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
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
mod render_tests;

#[cfg(test)]
mod scene_tests;

#[expect(
    clippy::cast_precision_loss,
    reason = "scene integer atmospheric units"
)]
pub(crate) fn height_fog(
    fog: worldrunner_render_plan::SceneHeightFog,
) -> worldrunner_render_lighting::atmosphere::HeightFog {
    worldrunner_render_lighting::atmosphere::HeightFog::new(
        fog.density_micro_per_meter as f32 / 1e6,
        fog.falloff_micro_per_meter as f32 / 1e6,
        fog.base_height_millimeters as f32 / 1000.0,
        fog.start_distance_millimeters as f32 / 1000.0,
        Vec3::from_array(fog.radiance_micro.map(|v| v as f32 / 1e6)),
    )
    .expect("integer atmospheric descriptor is finite and nonnegative")
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
    set
}

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod visibility_tests;
