#[cfg(test)]
#[path = "card_rejection_test_support.rs"]
mod card_rejection_test_support;
#[cfg(test)]
use crate::geometry::{instance_attributes, vertex_attributes};
#[cfg(test)]
use crate::pipeline::PipelineKey;
use crate::renderer::ScenePipelines;
use crate::shader::ENVIRONMENT_WGSL;
use worldrunner_render_gpu::resource::upload_buffer;
use worldrunner_render_gpu::submission::{FrameOwner, Publications};
#[cfg(test)]
use worldrunner_render_shadow::{DepthFragmentState, DepthPipelineDescriptor};
use worldrunner_render_shadow::{
    DepthMap, DepthMapBudget, DepthMapDescriptor, DepthMapError, ReceiverDescriptor,
    ReceiverDimension,
};

use super::DirectionalShadowSettings;

/// Bytes of the shadow uniform the shaders read at set 2, binding 0.
pub(crate) const SHADOW_UNIFORM_BYTES: usize = 352;

pub(super) const SHADOWED_WGSL: &str = concat!(
    worldrunner_render_shadow::shadow_wgsl!(),
    worldrunner_render_lighting::lighting_wgsl!(),
    worldrunner_render_post::post_wgsl!(),
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/ambient_visibility.wgsl"),
    crate::shader::decals_wgsl!("../shader/"),
    include_str!("../shader/camera_rays.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/geometry.wgsl"),
    include_str!("../shader/foliage.wgsl"),
    crate::shader::impostor_wgsl!("../shader/"),
    crate::shader::environment_wgsl!("../shader/")
);

const SHADOWED_TERRAIN_WGSL: &str = concat!(
    worldrunner_render_shadow::shadow_wgsl!(),
    worldrunner_render_lighting::lighting_wgsl!(),
    worldrunner_render_post::post_wgsl!(),
    include_str!("../shader/lighting.wgsl"),
    include_str!("../shader/ambient_visibility.wgsl"),
    crate::shader::decals_wgsl!("../shader/"),
    include_str!("../shader/camera_rays.wgsl"),
    include_str!("../shader/shadows.wgsl"),
    include_str!("../shader/terrain.wgsl")
);

pub(super) fn shadowed_terrain_source() -> &'static str {
    static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        crate::scene_metadata::receiver::terrain_local_source(SHADOWED_TERRAIN_WGSL)
    })
}

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

/// The object casters of `pipelines` for `settings` and `sampling`: the caster programs over
/// [`depth_source`], on the program runtime the receiver pipelines' objects draw with.
pub(super) fn object_caster(
    pipelines: &ScenePipelines,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> crate::object::program::Context {
    object_caster_from(
        pipelines,
        &depth_source(),
        caster_configuration(settings, sampling),
    )
}

/// The object casters of `pipelines` over the caster text `source`, with the depth test and bias
/// of `configuration`.
pub(super) fn object_caster_from(
    pipelines: &ScenePipelines,
    source: &str,
    configuration: worldrunner_render_shadow::DirectionalShadowConfig,
) -> crate::object::program::Context {
    let objects = pipelines.object_context();
    crate::object::program::Context::new(
        crate::object::program::Description {
            device: objects.device().clone(),
            source: source.into(),
            colors: Vec::new(),
            samples: 1,
            front_face: wgpu::FrontFace::Ccw,
            depth: crate::renderer::depth::DepthMode::Independent,
            previous_transforms: false,
            constants: Vec::new(),
            frame: pipelines.frame_layout().clone(),
            shadow: None,
            screen: None,
            entry: crate::object::program::Entry::Caster(crate::object::program::Caster {
                compare: configuration.caster_compare,
                bias: configuration.caster_bias,
            }),
        },
        objects.family(),
    )
}

#[cfg(test)]
pub(super) fn depth_pipelines(
    device: &wgpu::Device,
    pipelines: &ScenePipelines,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> [wgpu::RenderPipeline; crate::pipeline::PipelineKey::ALL.len()] {
    let configuration = caster_configuration(settings, sampling);
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
    // Relief is deliberately flat in directional maps, so its pipelines are identical.
    let objects: [_; 8] = std::array::from_fn(|index| {
        let key = PipelineKey::ALL[(index / 4) * 16 + index % 4];
        object_depth_pipeline(device, key, &layout, &module, &buffers, configuration)
    });
    std::array::from_fn(|index| {
        let key = PipelineKey::ALL[index];
        objects[usize::from(key.sampling.is_impostor()) * 4 + index % 4].clone()
    })
}

/// The caster policy of `settings` with the slope bias of the scene's shadow sampling.
pub(super) fn caster_configuration(
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> worldrunner_render_shadow::DirectionalShadowConfig {
    let mut configuration = settings.configuration();
    configuration.caster_bias.slope_scale = slope_bias(sampling);
    configuration
}

/// The terrain's caster for `settings`, drawing on the program runtime the receiver pipelines
/// share.
pub(super) fn terrain_caster(
    context: &crate::device::GpuContext,
    pipelines: &ScenePipelines,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
) -> super::terrain_caster::TerrainCaster {
    super::terrain_caster::TerrainCaster::new(
        &caster_configuration(settings, sampling),
        pipelines.terrain_context().family(),
        (context.device(), context.submission()),
        pipelines.frame_layout(),
    )
}

#[cfg(test)]
fn object_depth_pipeline(
    device: &wgpu::Device,
    key: PipelineKey,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    configuration: worldrunner_render_shadow::DirectionalShadowConfig,
) -> wgpu::RenderPipeline {
    worldrunner_render_shadow::create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: Some("environment"),
            layout,
            vertex: wgpu::VertexState {
                module,
                entry_point: Some("vs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: key.cull_mode(),
                ..Default::default()
            },
            compare: configuration.caster_compare,
            bias: configuration.caster_bias,
            fragment: Some(DepthFragmentState {
                module,
                entry_point: Some(if key.masked {
                    "fs_depth_masked"
                } else {
                    "fs_depth_opaque"
                }),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &key.fragment_constants(1),
                    ..Default::default()
                },
            }),
        },
    )
}

#[cfg(test)]
fn depth_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    let source = depth_source();
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shadow caster"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

pub(crate) fn depth_source() -> String {
    crate::renderer::depth::DepthMode::Independent
        .vertex_source(&format!(
            "{ENVIRONMENT_WGSL}\n{}",
            include_str!("../shader/shadow_depth.wgsl")
        ))
        .into_owned()
}

pub(super) struct ShadowMap {
    pub depth: DepthMap,
    /// Detached when replaced; recorded frames keep the census entry through completion.
    census_owner: Option<FrameOwner<worldrunner_render_gpu::census::CensusEntry>>,
    pub uniform: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    slots: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    pub(super) sampler: wgpu::Sampler,
}

impl std::fmt::Debug for ShadowMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShadowMap")
            .field("depth", &self.depth)
            .field("census_owned", &self.census_owner.is_some())
            .field("uniform", &self.uniform)
            .field("group", &self.group)
            .field("slots", &self.slots)
            .field("sampler", &self.sampler)
            .finish()
    }
}

impl ShadowMap {
    /// Set 2 of the receivers that sample this map, with the Opaque depth when relief cards
    /// reject against it.
    pub fn receiver_groups(
        &self,
        scheduler: &crate::compositor::FrameRenderer,
        device: &wgpu::Device,
        card_depth: Option<&worldrunner_render_post::AttachmentView>,
    ) -> crate::scene_metadata::receiver::SetTwo {
        scheduler.shadow_groups(
            device,
            (
                &self.group,
                crate::scene_metadata::receiver::ShadowInputs {
                    uniform: &self.uniform,
                    depth: self.depth.receiver_view(),
                    sampler: &self.sampler,
                },
            ),
            card_depth,
        )
    }

    pub fn select_frame_slot(&mut self, slot: usize) {
        self.uniform = self.slots[slot].0.clone();
        self.group = self.slots[slot].1.clone();
    }

    pub fn retain(
        &self,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<(), worldrunner_render_gpu::submission::SubmitError> {
        let owner = self.census_owner.as_ref().expect("live shadow map owner");
        Publications::retain_lease(encoder, &owner.lease())
    }

    pub fn new(
        context: &crate::device::GpuContext,
        settings: DirectionalShadowSettings,
        layout: &wgpu::BindGroupLayout,
        layers: u32,
    ) -> Result<Self, DepthMapError> {
        let device = context.device();
        let depth = shadow_depth_map(device, settings, layers)?;
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
        let census = context.submission().census().track(
            worldrunner_render_gpu::census::ResourceClass::ShadowMap,
            worldrunner_render_gpu::census::texture_bytes(depth.texture()),
        );
        let census_owner = FrameOwner::new(census, context.submission());
        Ok(Self {
            depth,
            census_owner: Some(census_owner),
            uniform: slots[0].0.clone(),
            group: slots[0].1.clone(),
            slots,
            sampler,
        })
    }
}

impl Drop for ShadowMap {
    fn drop(&mut self) {
        if let Some(owner) = self.census_owner.take() {
            owner.detach();
        }
    }
}

fn shadow_depth_map(
    device: &wgpu::Device,
    settings: DirectionalShadowSettings,
    layers: u32,
) -> Result<DepthMap, DepthMapError> {
    let maximum = u64::from(super::MAX_SHADOW_RESOLUTION);
    DepthMap::new(
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
                max_texture_bytes: maximum * maximum * 4 * 4,
                max_readback_bytes: if cfg!(test) { maximum * maximum * 4 } else { 0 },
            },
        },
    )
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
    let reduction = strength_reduction(sampling);
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
        cascades.selection().uniform_word(),
        cascades.near,
    ]);
    values.extend(cascades.splits);
    values.extend(cascades.view_depth.to_array());
    values.extend(cascades.depth_bias);
    values.extend([cascades.fade_out, 0.0, 0.0, 0.0]);
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
        crate::renderer::SceneTarget { format, samples },
        &source,
        &terrain_source,
        Some(shadow_layout),
        reflection_layout.as_ref().or(screen_layout.as_ref()),
    );

    pipelines.ssao = screen_layout.is_some();
    Ok(pipelines)
}

/// The shadow receiver object and terrain sources for the enabled screen-space features.
pub(crate) fn receiver_sources(ssao: bool, reflection: bool, velocity: bool) -> (String, String) {
    if reflection {
        return crate::ssr::forward::velocity_sources(
            (SHADOWED_WGSL, shadowed_terrain_source()),
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
        crate::velocity::terrain_source(shadowed_terrain_source())
    } else {
        shadowed_terrain_source().to_owned()
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

/// How much less than the whole shadow the receivers apply: the uniform stores it, and the
/// receivers apply `1 - ` it.
pub(super) fn strength_reduction(sampling: worldrunner_render_plan::SceneShadowSampling) -> f32 {
    micro_unit(1_000_000 - sampling.strength_micro)
}

fn micro_unit(value: u32) -> f32 {
    let high = f32::from(u16::try_from(value / 1000).expect("validated micro-units"));
    let low = f32::from(u16::try_from(value % 1000).expect("micro-unit remainder"));
    (high * 1000.0 + low) / 1_000_000.0
}
