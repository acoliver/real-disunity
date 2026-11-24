//! Allocation of a complete stock renderer generation.
use super::{RendererConfiguration, install_jitter};
use crate::compositor::game::TargetStore;
use crate::compositor::{FrameOutput, FrameRenderer};
use crate::device::GpuContext;
use worldrunner_render_post::{PostChain, PostError, Resources, ToneMapOutput};

/// What a generation takes over from the one it replaces: the effect history, the timing state
/// and the store of the game's targets. None of it is changed by building the replacement.
#[derive(Default)]
pub(super) struct Carried<'a> {
    pub history: Option<&'a ToneMapOutput>,
    pub timing: Option<worldrunner_render_post::TimingState>,
    pub game: Option<&'a TargetStore>,
}

pub(super) fn allocate(
    context: &GpuContext,
    settings: &RendererConfiguration,
    carried: Carried<'_>,
) -> Result<FrameRenderer, PostError> {
    let scope = worldrunner_render_gpu::configuration::AllocationScope::new(context.device());
    let result = build(context, settings, carried);
    scope.finish()?;
    result
}

fn build(
    context: &GpuContext,
    settings: &RendererConfiguration,
    carried: Carried<'_>,
) -> Result<FrameRenderer, PostError> {
    let factory = output(context, settings, carried.history)?;
    let mut renderer = new_renderer(context, settings, factory)?;
    finish_renderer(&mut renderer, context, settings, carried)?;
    Ok(renderer)
}

fn output(
    context: &GpuContext,
    settings: &RendererConfiguration,
    history: Option<&ToneMapOutput>,
) -> Result<FrameOutput, PostError> {
    let (exposure, tone) = settings.hdr;
    Ok(FrameOutput::Hdr(
        ToneMapOutput::new(settings.output_format, exposure, tone)?.with_retained_effects(
            context.device(),
            settings.post,
            history,
        )?,
    ))
}

fn new_renderer(
    context: &GpuContext,
    settings: &RendererConfiguration,
    factory: FrameOutput,
) -> Result<FrameRenderer, PostError> {
    let device = context.device();
    let metadata = settings.prepass_metadata();
    Ok(FrameRenderer {
        contours: crate::contours::Targets::reserved(settings.contour_ids),
        contour_format: context
            .adapter()
            .get_texture_format_features(crate::contours::FORMAT),
        overlay: super::super::overlay::OverlayStage::default(),
        output_format: settings.output_format,
        plan_cache: std::rc::Rc::default(),
        game: crate::compositor::game::Installed::default(),
        game_camera: super::super::FrameCameraHandle::default(),
        game_terrain: super::super::FrameTerrainHandle::default(),
        game_textures: super::super::game::FrameSceneTexturesHandle::default(),
        chain: scene_chain(context, settings, factory)?,
        environment_conventions: [None; 2],
        water: None,
        water_state: std::rc::Rc::default(),
        snapshots: None,
        snapshot_ready: std::rc::Rc::default(),
        frame_slot: 0,
        clear_color: settings.clear_color,
        post_requested: settings.post,
        elapsed_seconds: 0.0,
        selected_time: None,
        receiver_gradients: settings.receiver_gradients,
        #[cfg(test)]
        receiver_reference: None,
        animation_time: None,
        frame_delta: 0.0,
        post_time: std::rc::Rc::default(),
        exposure_reset: std::rc::Rc::default(),
        temporal: std::rc::Rc::default(),
        ssao: metadata
            .map(|prepass| prepass.metadata(device, context.queue(), settings.frame.size))
            .transpose()?,
        ssr: None,
        ssao_requested: settings.ssao,
        ssr_requested: settings.ssr,
    })
}

fn scene_chain(
    context: &GpuContext,
    settings: &RendererConfiguration,
    factory: FrameOutput,
) -> Result<PostChain<FrameOutput>, PostError> {
    let device = context.device();
    PostChain::new(
        device,
        settings.frame,
        Resources::default(),
        factory,
        super::super::depth_reuse::allocation(
            device.features(),
            settings.frame,
            settings.prepass_metadata().is_some(),
        ),
    )
}

fn finish_renderer(
    renderer: &mut FrameRenderer,
    context: &GpuContext,
    settings: &RendererConfiguration,
    carried: Carried<'_>,
) -> Result<(), PostError> {
    let device = context.device();
    renderer.install_reflections(device)?;
    renderer.share_metadata_depth(device);
    install_jitter(renderer, settings)?;
    // The inventory installs first: what its factories draw decides whether the stock water
    // registers.
    settings
        .game_passes
        .install(renderer, context, carried.game)?;
    renderer.install_water_bodies(device, context.submission(), settings.water.clone())?;
    ensure_snapshots(renderer, device, settings);
    renderer.ensure_game_snapshots(device);
    restore_timing(renderer, context, settings.timing, carried.timing)?;
    Ok(())
}

fn ensure_snapshots(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    settings: &RendererConfiguration,
) {
    if (settings.ui_depth || renderer.ssr.is_some()) && renderer.snapshots.is_none() {
        renderer.snapshots = Some(std::array::from_fn(|_| {
            crate::water::snapshots::Snapshots::new(device, settings.frame)
        }));
    }
}

fn restore_timing(
    renderer: &mut FrameRenderer,
    context: &GpuContext,
    mode: worldrunner_render_post::TimingMode,
    timing: Option<worldrunner_render_post::TimingState>,
) -> Result<(), PostError> {
    if let Some(timing) = timing {
        renderer.chain.restore_timing(timing);
    } else {
        renderer
            .configure_timing(context.device(), context.queue(), mode)
            .map_err(|error| PostError::Allocation(error.to_string()))?;
    }
    Ok(())
}
