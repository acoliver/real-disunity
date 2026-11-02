use super::Renderer;
use crate::{
    AdapterCallback, FrameProfile, FrameSettings, PresentationBudget, RetainedHost, ToneMap,
    ViewerError,
};
use worldrunner_render_gpu::configuration::ConfigurationError;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};

pub(super) fn selected_frame() -> FrameSettings {
    FrameSettings {
        shadows: None,
        frame_profile: FrameProfile::HdrUpscaled {
            samples: 4,
            tone_map: ToneMap::Reinhard,
            scale: RenderScale::new(500_000).unwrap(),
            upscale: UpscaleSettings {
                mode: UpscaleMode::Easu,
                sharpness: None,
            },
        },
        presentation_budget: PresentationBudget(300 * 1024 * 1024).into(),
    }
}

#[test]
fn adapter_frame_windowless_targets_use_selected_samples_and_scale_from_first_frame() {
    let mut options = crate::tests::options();
    options.size = [64, 32];
    options.adapter_callback = Some(AdapterCallback::new(|_| Ok(Some(selected_frame()))));
    let mut host = RetainedHost::new(crate::tests::scene(false), options).unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host)).unwrap();
    let frame = renderer.presentation.scheduler.configuration().frame;
    assert_eq!(frame.samples, 4);
    assert_eq!(frame.size, [32, 16]);
    assert_eq!(frame.output_extent(), [64, 32]);
    assert_eq!(host.options().frame_profile, selected_frame().frame_profile);
    assert!(renderer.render_retained(&mut host).unwrap());
    assert_eq!(renderer.presentation.scheduler.configuration().frame, frame);
    renderer.drain().unwrap();
}

#[test]
fn adapter_frame_windowless_rejects_invalid_samples_before_allocation() {
    let mut options = crate::tests::options();
    options.size = [8, 8];
    options.adapter_callback = Some(AdapterCallback::new(|_| {
        Ok(Some(FrameSettings {
            frame_profile: FrameProfile::Hdr {
                samples: 16,
                tone_map: ToneMap::Reinhard,
            },
            ..selected_frame()
        }))
    }));
    let mut host = RetainedHost::new(crate::tests::scene(false), options).unwrap();
    assert!(matches!(
        pollster::block_on(Renderer::windowless(&mut host)),
        Err(ViewerError::Configuration(ConfigurationError::Allocation(
            worldrunner_render_post::PostError::SampleCount(16)
        )))
    ));
}

#[test]
fn adapter_frame_windowless_rejects_selected_budget_before_residency() {
    let mut options = crate::tests::options();
    options.size = [8, 8];
    options.adapter_callback = Some(AdapterCallback::new(|_| {
        Ok(Some(FrameSettings {
            presentation_budget: PresentationBudget(1).into(),
            ..selected_frame()
        }))
    }));
    let mut host = RetainedHost::new(crate::tests::scene(false), options).unwrap();
    assert!(matches!(
        pollster::block_on(Renderer::windowless(&mut host)),
        Err(ViewerError::Configuration(ConfigurationError::Budget {
            budget: 1,
            ..
        }))
    ));
}
