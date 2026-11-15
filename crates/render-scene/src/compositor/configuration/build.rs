//! Allocation of a complete stock renderer generation.
use super::{
    FrameOutput, FrameRenderer, GpuContext, PostChain, PostError, RendererConfiguration, Resources,
    ToneMapOutput, install_jitter,
};

pub(super) fn allocate(
    context: &GpuContext,
    settings: &RendererConfiguration,
    history: Option<&ToneMapOutput>,
    timing: Option<worldrunner_render_post::TimingState>,
) -> Result<FrameRenderer, PostError> {
    let device = context.device();
    let scope = worldrunner_render_gpu::configuration::AllocationScope::new(device);
    let result = build(context, settings, history, timing);
    scope.finish()?;
    result
}

fn build(
    context: &GpuContext,
    settings: &RendererConfiguration,
    history: Option<&ToneMapOutput>,
    timing: Option<worldrunner_render_post::TimingState>,
) -> Result<FrameRenderer, PostError> {
    let device = context.device();
    let factory = if let Some((exposure, tone)) = settings.hdr {
        FrameOutput::Hdr(
            ToneMapOutput::new(settings.output_format, exposure, tone)?.with_retained_effects(
                device,
                settings.post,
                history,
            )?,
        )
    } else {
        FrameOutput::Copy(worldrunner_render_post::CopyOutput::new(
            device,
            settings.output_format,
        ))
    };
    let metadata = settings.prepass_metadata();
    let mut renderer = FrameRenderer {
        contours: crate::contours::Targets::reserved(settings.contour_ids),
        contour_format: context
            .adapter()
            .get_texture_format_features(crate::contours::FORMAT),
        overlay: crate::compositor::overlay::OverlayStage::default(),
        output_format: settings.output_format,
        chain: PostChain::with_scene_depth(
            device,
            settings.frame,
            Resources::default(),
            factory,
            crate::compositor::depth_reuse::allocation(
                device.features(),
                settings.frame,
                metadata.is_some(),
            ),
        )?,
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
            .map(|ssao| {
                crate::scene_metadata::SceneMetadata::new(device, settings.frame.size, ssao)
            })
            .transpose()?,
        ssr: None,
        ssao_requested: settings.ssao,
        ssr_requested: settings.ssr,
        gbuffer: crate::compositor::opaque::allocate_path(
            device,
            settings.opaque_path,
            settings.frame,
        )?,
    };
    renderer.install_reflections(device)?;
    renderer.install_gbuffer_producer(device);
    renderer.share_metadata_depth(device);
    install_jitter(&mut renderer, settings)?;
    renderer.install_water_bodies(device, context.submission(), settings.water.clone())?;
    if (settings.ui_depth || renderer.ssr.is_some()) && renderer.snapshots.is_none() {
        renderer.snapshots = Some(std::array::from_fn(|_| {
            crate::water::snapshots::Snapshots::new(device, settings.frame)
        }));
    }
    if let Some(timing) = timing {
        renderer.chain.restore_timing(timing);
    } else {
        renderer
            .configure_timing(device, context.queue(), settings.timing)
            .map_err(|error| PostError::Allocation(error.to_string()))?;
    }
    Ok(renderer)
}
