use super::{FrameOutput, FrameRenderer, depth_reuse, overlay};
use crate::device::GpuContext;
use worldrunner_render_post::{
    AntiAliasing, FrameConfig, HdrPostSettings, PostError, ToneMapOutput,
};

struct HdrRequest<'a> {
    context: &'a GpuContext,
    size: [u32; 2],
    output_format: wgpu::TextureFormat,
    antialiasing: AntiAliasing,
    tone_map: worldrunner_render_post::ToneMap,
    effects: Option<&'a worldrunner_render_plan::SceneEffects>,
}

struct HdrSetup {
    config: FrameConfig,
    post: HdrPostSettings,
    ssao_requested: Option<worldrunner_render_plan::SceneSsao>,
    ssr_requested: Option<worldrunner_render_plan::SceneSsr>,
    metadata: Option<depth_reuse::Prepass>,
    factory: ToneMapOutput,
}

pub(super) fn build(
    context: &GpuContext,
    size: [u32; 2],
    output_format: wgpu::TextureFormat,
    antialiasing: AntiAliasing,
    tone_map: worldrunner_render_post::ToneMap,
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> Result<FrameRenderer, PostError> {
    let request = HdrRequest {
        context,
        size,
        output_format,
        antialiasing,
        tone_map,
        effects,
    };
    let setup = allocate_hdr(&request)?;
    construct_renderer(&request, setup)
}

fn allocate_hdr(request: &HdrRequest<'_>) -> Result<HdrSetup, PostError> {
    let post = validate_hdr(request)?;
    let config = request
        .antialiasing
        .frame_config(request.size, wgpu::TextureFormat::Rgba16Float);
    let exposure = request.effects.map_or(1.0, |effects| {
        f64::from(effects.grading().exposure_micro) / 1_000_000.0
    });
    let factory = ToneMapOutput::new(request.output_format, exposure, request.tone_map)?
        .with_effects(request.context.device(), post)?;
    let ssao_requested = request.effects.and_then(|effects| effects.ssao()).copied();
    let ssr_requested = request.effects.and_then(|effects| effects.ssr()).copied();
    let metadata = depth_reuse::prepass_metadata(
        ssao_requested,
        ssr_requested.as_ref(),
        request
            .effects
            .is_some_and(|effects| effects.depth_fitting().is_some()),
    );
    Ok(HdrSetup {
        config,
        post,
        ssao_requested,
        ssr_requested,
        metadata,
        factory,
    })
}

fn validate_hdr(request: &HdrRequest<'_>) -> Result<HdrPostSettings, PostError> {
    request.antialiasing.validate()?;
    let post = crate::post::settings_with_antialiasing(request.effects, request.antialiasing)?;
    super::validation::validate_hdr_effects(request.effects)?;
    let config = request
        .antialiasing
        .frame_config(request.size, wgpu::TextureFormat::Rgba16Float);
    config.validate_adapter(request.context.adapter())?;
    super::validate_reflection_extent(
        request.context.device(),
        config,
        request.effects.and_then(|effects| effects.ssr()),
    )?;
    post.validate_frame(config)?;
    Ok(post)
}

fn construct_renderer(
    request: &HdrRequest<'_>,
    setup: HdrSetup,
) -> Result<FrameRenderer, PostError> {
    let device = request.context.device();
    let mut renderer = FrameRenderer {
        contours: crate::contours::Targets::default(),
        contour_format: request
            .context
            .adapter()
            .get_texture_format_features(crate::contours::FORMAT),
        overlay: overlay::OverlayStage::default(),
        output_format: request.output_format,
        environment_conventions: [None; 2],
        ssao_requested: setup.ssao_requested,
        ssr_requested: setup.ssr_requested,
        plan_cache: std::rc::Rc::default(),
        game: super::game::Installed::default(),
        game_camera: super::FrameCameraHandle::default(),
        game_terrain: super::FrameTerrainHandle::default(),
        game_textures: super::game::FrameSceneTexturesHandle::default(),
        ssr: None,
        water: None,
        water_state: std::rc::Rc::default(),
        snapshots: None,
        snapshot_ready: std::rc::Rc::default(),
        frame_slot: 0,
        clear_color: wgpu::Color::BLACK,
        post_requested: setup.post,
        elapsed_seconds: 0.0,
        selected_time: None,
        receiver_gradients: super::ReceiverGradientReservation::Opposing,
        #[cfg(test)]
        receiver_reference: None,
        animation_time: None,
        frame_delta: 0.0,
        post_time: std::rc::Rc::new(std::cell::Cell::new(0.0)),
        exposure_reset: std::rc::Rc::new(std::cell::Cell::new(false)),
        temporal: std::rc::Rc::default(),
        ssao: setup
            .metadata
            .map(|prepass| prepass.metadata(device, request.context.queue(), request.size))
            .transpose()?,
        chain: worldrunner_render_post::PostChain::new(
            device,
            setup.config,
            worldrunner_render_post::Resources::default(),
            FrameOutput::Hdr(setup.factory),
            depth_reuse::allocation(device.features(), setup.config, setup.metadata.is_some()),
        )?,
    };
    finish_renderer(&mut renderer, request, &setup.post, setup.config)?;
    Ok(renderer)
}

fn finish_renderer(
    renderer: &mut FrameRenderer,
    request: &HdrRequest<'_>,
    post: &HdrPostSettings,
    config: FrameConfig,
) -> Result<(), PostError> {
    renderer.install_reflections(request.context.device())?;
    renderer.share_metadata_depth(request.context.device());
    renderer.set_projection_jitter(crate::post::jitter(post, config)?)?;
    if let Some(effects) = request.effects {
        renderer.install_water_bodies(
            request.context.device(),
            request.context.submission(),
            effects.water_bodies().to_vec(),
        )?;
    }
    Ok(())
}
