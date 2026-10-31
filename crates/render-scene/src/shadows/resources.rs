use crate::geometry::{instance_attributes, vertex_attributes};
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::shader::ENVIRONMENT_WGSL;
use crate::upload::upload_buffer;
use worldrunner_render_shadow::{
    DepthFragmentState, DepthMap, DepthMapBudget, DepthMapDescriptor, DepthMapError,
    DepthPipelineDescriptor, ReceiverDescriptor, ReceiverDimension,
};

use super::DirectionalShadowSettings;

const SHADOW_UNIFORM_BYTES: usize = 336;

pub(super) const SHADOWED_WGSL: &str = concat!(
    worldrunner_render_shadow::shadow_wgsl!(),
    worldrunner_render_lighting::lighting_wgsl!(),
    worldrunner_render_post::post_wgsl!(),
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/ambient_visibility.wgsl"),
    include_str!("../shader/decals.wgsl"),
    include_str!("../shader/camera_rays.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/geometry.wgsl"),
    include_str!("../shader/foliage.wgsl"),
    include_str!("../shader/impostor.wgsl"),
    include_str!("../shader/environment.wgsl")
);

pub(super) const SHADOWED_TERRAIN_WGSL: &str = concat!(
    worldrunner_render_shadow::shadow_wgsl!(),
    worldrunner_render_lighting::lighting_wgsl!(),
    worldrunner_render_post::post_wgsl!(),
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/ambient_visibility.wgsl"),
    include_str!("../shader/decals.wgsl"),
    include_str!("../shader/camera_rays.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/terrain.wgsl")
);

pub(super) fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    worldrunner_render_shadow::receiver_layout(
        device,
        ReceiverDescriptor {
            label: Some("directional shadow"),
            dimension: ReceiverDimension::D2Array,
            texture_binding: 1,
            sampler_binding: 2,
            uniform_binding: 0,
            min_uniform_binding_size: None,
        },
    )
}

pub(super) fn depth_pipelines(
    device: &wgpu::Device,
    pipelines: &ScenePipelines,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> ([wgpu::RenderPipeline; 16], wgpu::RenderPipeline) {
    let mut configuration = settings.configuration();
    configuration.caster_bias.slope_scale = slope_bias(sampling);
    let module = depth_shader(device);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shadow caster"),
        bind_group_layouts: &[
            Some(pipelines.frame_layout()),
            Some(pipelines.material_layout()),
        ],
        immediate_size: 0,
    });
    let vertices = vertex_attributes();
    let instances = instance_attributes();
    let buffers = crate::renderer::vertex_buffers(&vertices, &instances);
    let terrain = terrain_depth_pipeline(
        device,
        &module,
        pipelines.frame_layout(),
        &buffers,
        configuration,
    );
    // Relief is deliberately flat in directional maps, so its pipelines are identical.
    let objects: [_; 4] = std::array::from_fn(|index| {
        let key = PipelineKey::ALL[index];
        worldrunner_render_shadow::create_depth_pipeline(
            device,
            DepthPipelineDescriptor {
                label: Some("environment"),
                layout: &layout,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_shadow"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: key.cull_mode(),
                    ..Default::default()
                },
                compare: configuration.caster_compare,
                bias: configuration.caster_bias,
                fragment: Some(DepthFragmentState {
                    module: &module,
                    entry_point: Some(if key.masked {
                        "fs_depth_masked"
                    } else {
                        "fs_depth_opaque"
                    }),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
            },
        )
    });
    (
        std::array::from_fn(|index| objects[index % 4].clone()),
        terrain,
    )
}

fn terrain_depth_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    frame_layout: &wgpu::BindGroupLayout,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    configuration: worldrunner_render_shadow::DirectionalShadowConfig,
) -> wgpu::RenderPipeline {
    let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain shadow caster"),
        bind_group_layouts: &[Some(frame_layout)],
        immediate_size: 0,
    });
    worldrunner_render_shadow::create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: Some("terrain"),
            layout: &terrain_layout,
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_terrain_depth"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            compare: configuration.caster_compare,
            bias: configuration.caster_bias,
            fragment: None,
        },
    )
}

fn depth_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    let source = format!(
        "{ENVIRONMENT_WGSL}\n{}",
        include_str!("../shader/shadow_depth.wgsl")
    );
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow caster"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

#[derive(Debug)]
pub(super) struct ShadowMap {
    pub depth: DepthMap,
    pub uniform: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    slots: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
}

impl ShadowMap {
    pub fn select_frame_slot(&mut self, slot: usize) {
        self.uniform = self.slots[slot].0.clone();
        self.group = self.slots[slot].1.clone();
    }
    pub fn new(
        context: &crate::device::GpuContext,
        settings: DirectionalShadowSettings,
        layout: &wgpu::BindGroupLayout,
        layers: u32,
    ) -> Result<Self, DepthMapError> {
        let device = context.device();
        let max_resolution = u64::from(super::MAX_SHADOW_RESOLUTION);
        let depth = DepthMap::new(
            device,
            DepthMapDescriptor {
                label: Some("directional shadow depth"),
                resolution: settings.resolution,
                layers,
                receiver_dimension: ReceiverDimension::D2Array,
                readback: cfg!(test),
                budget: DepthMapBudget {
                    max_resolution: super::MAX_SHADOW_RESOLUTION,
                    max_layers: 4,
                    max_texture_bytes: max_resolution * max_resolution * 4 * 4,
                    max_readback_bytes: if cfg!(test) {
                        max_resolution * max_resolution * 4
                    } else {
                        0
                    },
                },
            },
        )?;
        let sampler = settings.configuration().comparison_sampler(device);
        let slots: Vec<_> = (0..3)
            .map(|_| {
                let uniform = upload_buffer(
                    device,
                    context.queue(),
                    "shadow uniform",
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
                    &[0; SHADOW_UNIFORM_BYTES],
                );
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("directional shadow"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(depth.receiver_view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                });
                (uniform, group)
            })
            .collect();
        Ok(Self {
            depth,
            uniform: slots[0].0.clone(),
            group: slots[0].1.clone(),
            slots,
        })
    }
}

#[cfg(test)]
pub(super) fn uniform_bytes(
    cascades: &super::cascades::CascadeViews,
    settings: DirectionalShadowSettings,
) -> Vec<u8> {
    uniform_bytes_with_sampling(
        cascades,
        settings,
        worldrunner_render_plan::SceneShadowSampling::default(),
    )
}

pub(super) fn slope_bias(sampling: worldrunner_render_plan::SceneShadowSampling) -> f32 {
    micro_unit(sampling.caster_slope_bias_micro)
}

pub(super) fn uniform_bytes_with_sampling(
    cascades: &super::cascades::CascadeViews,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> Vec<u8> {
    let mut values = Vec::with_capacity(SHADOW_UNIFORM_BYTES / 4);
    for index in 0..4 {
        values.extend(
            cascades
                .matrices
                .get(index)
                .copied()
                .unwrap_or(glam::Mat4::IDENTITY)
                .to_cols_array(),
        );
    }
    let footprint = match sampling.filter {
        worldrunner_render_plan::SceneShadowFilter::Grid { radius } => {
            f32::from(u16::try_from(radius).expect("validated radius"))
        }
        worldrunner_render_plan::SceneShadowFilter::Bilinear2 => -1.0,
    };
    // The unused word stores reduction so the default uniform bytes stay unchanged.
    let reduction = micro_unit(1_000_000 - sampling.strength_micro);
    values.extend([
        cascades.multiplier,
        settings.normal_bias_meters,
        footprint,
        reduction,
    ]);
    values.extend([
        if cascades.no_receivers {
            0.0
        } else {
            f32::from(u16::try_from(cascades.matrices.len()).expect("four cascades"))
        },
        cascades.blend,
        if cascades.recurrence.is_some() {
            2.0
        } else {
            f32::from(u8::from(cascades.camera_splits))
        },
        cascades.near,
    ]);
    values.extend(cascades.splits);
    values.extend(cascades.view_depth.to_array());
    values.extend(cascades.depth_bias);
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

pub(super) fn depth_frames(
    context: &crate::device::GpuContext,
    layout: &wgpu::BindGroupLayout,
) -> Vec<(wgpu::Buffer, wgpu::BindGroup)> {
    use crate::frame::FRAME_UNIFORM_BYTES;
    (0..4)
        .map(|_| {
            let buffer = upload_buffer(
                context.device(),
                context.queue(),
                "shadow frame",
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_SRC,
                &[0; FRAME_UNIFORM_BYTES],
            );
            let group = crate::draw::decals::empty_group(context.device(), layout, &buffer);
            (buffer, group)
        })
        .collect()
}

pub(super) fn receiver_pipelines(
    context: &crate::device::GpuContext,
    scene: &worldrunner_render_plan::RenderScene,
    target: (wgpu::TextureFormat, u32, bool),
    shadow_layout: &wgpu::BindGroupLayout,
) -> Result<ScenePipelines, super::ShadowError> {
    let (format, samples, velocity) = target;
    let screen_layout = scene
        .effects()
        .and_then(|effects| effects.ssao())
        .map(|_| crate::ssao::ambient_layout(context.device()));
    if screen_layout.is_some() && format != wgpu::TextureFormat::Rgba16Float {
        return Err(crate::frame::FrameError::UnsupportedEffect("SSAO").into());
    }
    let reflection = scene
        .effects()
        .and_then(|effects| effects.ssr())
        .is_some_and(|ssr| ssr.enabled);
    // Reflection receivers keep camera reprojection when the colour budget has no room for it.
    let velocity = velocity
        && (!reflection
            || crate::velocity::reflection_writes_velocity(context.device(), format, true));
    let (source, terrain_source) = receiver_sources(screen_layout.is_some(), reflection, velocity);
    let reflection_layout = reflection.then(|| crate::ssr::forward::layout(context.device()));
    let mut pipelines = ScenePipelines::with_screen_space(
        context.device(),
        (format, samples),
        &source,
        &terrain_source,
        Some(shadow_layout),
        reflection_layout.as_ref().or(screen_layout.as_ref()),
    );

    pipelines.ssao = screen_layout.is_some();
    Ok(pipelines)
}

/// The shadow receiver object and terrain sources for the enabled screen-space features.
fn receiver_sources(ssao: bool, reflection: bool, velocity: bool) -> (String, String) {
    if reflection {
        return crate::ssr::forward::velocity_sources(
            (SHADOWED_WGSL, SHADOWED_TERRAIN_WGSL),
            velocity,
            false,
        );
    }
    let object = if velocity {
        crate::velocity::object_source(SHADOWED_WGSL)
    } else {
        SHADOWED_WGSL.to_owned()
    };
    let terrain = if velocity {
        crate::velocity::terrain_source(SHADOWED_TERRAIN_WGSL)
    } else {
        SHADOWED_TERRAIN_WGSL.to_owned()
    };
    if ssao {
        (
            crate::ssao::ambient_source(&object),
            crate::ssao::ambient_source(&terrain),
        )
    } else {
        (object, terrain)
    }
}

fn micro_unit(value: u32) -> f32 {
    let high = f32::from(u16::try_from(value / 1000).expect("validated micro-units"));
    let low = f32::from(u16::try_from(value % 1000).expect("micro-unit remainder"));
    (high * 1000.0 + low) / 1_000_000.0
}
