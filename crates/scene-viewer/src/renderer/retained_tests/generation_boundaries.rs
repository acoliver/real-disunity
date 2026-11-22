use super::offscreen::clearing_operations;
use super::*;
use crate::{CaptureJitter, RetainedCaptureRecipe, RetainedHost, RetainedUpdate};

#[test]
fn capture_hold_keeps_queued_fitting_disable_from_admitting_offscreen_work() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let scene = crate::renderer::sdsm_tests::fitted_scene();
    let mut host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [64, 64],
            shadows: Some(shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    host.request_capture(0).unwrap();
    host.apply(RetainedUpdate::Settings(Box::new(
        source().effects().unwrap().clone(),
    )))
    .unwrap();
    assert!(host.captures.holds_generation(0));
    renderer.publish_retained(&mut host).unwrap();
    assert_eq!(host.ready_generation(), 0);
    assert_eq!(host.accepted_generation(), 1);
    assert!(renderer.resident.depth_fitting_enabled());

    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, _, _runtime) = clearing_operations(&device, &authority);
    let serial = authority.submitted_serial();
    host.admit_offscreen(operations);
    assert!(matches!(
        renderer.render_retained(&mut host),
        Err(crate::ViewerError::UnsupportedScene(
            "player offscreen work with same-frame depth fitting"
        ))
    ));
    assert_eq!(authority.submitted_serial(), serial);
    assert!(host.offscreen.is_some());
    assert_eq!(host.ready_generation(), 0);
}

#[test]
fn offscreen_work_added_between_accumulated_samples_is_rejected_while_fitted() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut host = RetainedHost::new(
        crate::renderer::sdsm_tests::fitted_scene(),
        crate::ViewerOptions {
            size: [64, 64],
            shadows: Some(shadow_settings()),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    let recipe = RetainedCaptureRecipe::new(
        1,
        2,
        CaptureJitter::Halton23,
        false,
        vec![crate::CaptureOutput::Srgb8],
        4 * 1024 * 1024,
    )
    .unwrap();
    host.request_capture_recipe(0, recipe).unwrap();
    assert!(!renderer.render_retained(&mut host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    assert!(host.captures.holds_generation(0));
    assert!(renderer.streaming.captures.active.is_some());

    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, _, _runtime) = clearing_operations(&device, &authority);
    let serial = authority.submitted_serial();
    host.admit_offscreen(operations);
    assert!(matches!(
        renderer.render_retained(&mut host),
        Err(crate::ViewerError::UnsupportedScene(
            "player offscreen work with same-frame depth fitting"
        ))
    ));
    assert_eq!(authority.submitted_serial(), serial);
    assert!(host.offscreen.is_some());
    assert!(renderer.streaming.captures.active.is_some());
}
