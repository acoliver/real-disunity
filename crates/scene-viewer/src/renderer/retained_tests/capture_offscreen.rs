use super::offscreen::{clearing_operations, is_cleared};
use super::*;
use crate::{CaptureJitter, CaptureOutput, RetainedCaptureRecipe, RetainedHost};

fn capture_offscreen(ordinary_prewarm_frames: usize) {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [64, 64],
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    for _ in 0..ordinary_prewarm_frames {
        assert!(renderer.render_retained(&mut host).unwrap());
        renderer.drain().unwrap();
    }
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, image, _runtime) = clearing_operations(&device, &authority);
    host.admit_offscreen(operations);
    let id = host
        .request_capture_recipe(
            0,
            RetainedCaptureRecipe::new(
                1,
                2,
                CaptureJitter::Halton23,
                false,
                vec![CaptureOutput::Srgb8],
                4 * 1024 * 1024,
            )
            .unwrap(),
        )
        .unwrap();
    let before = authority.submitted_serial();

    assert!(!renderer.render_retained(&mut host).unwrap());
    assert_eq!(authority.submitted_serial(), before + 1);
    assert!(
        host.offscreen.is_none(),
        "the admitted sample took the packet"
    );
    assert!(host.captures.waiting.iter().any(|request| request.0 == id));
    renderer.drain().unwrap();
    assert!(is_cleared(&image.bytes(&device)));

    let after_first = authority.submitted_serial();
    assert!(!renderer.render_retained(&mut host).unwrap());
    assert_eq!(authority.submitted_serial(), after_first + 1);
    assert!(host.offscreen.is_none(), "the packet is not replayed");
}

#[test]
fn accumulated_capture_offscreen_packet_is_handed_off_without_ordinary_prewarm() {
    capture_offscreen(0);
}

#[test]
fn accumulated_capture_offscreen_packet_is_handed_off_after_ordinary_prewarm() {
    capture_offscreen(1);
}

#[test]
fn accumulated_capture_keeps_offscreen_packet_when_admission_is_blocked() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [64, 64],
            frames_in_flight: 2,
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut renderer = pollster::block_on(Renderer::windowless(&mut host, None)).unwrap();
    let device = renderer.context.device().clone();
    let authority = renderer.context.submission().clone();
    let (operations, image, _runtime) = clearing_operations(&device, &authority);
    host.admit_offscreen(operations);
    host.request_capture_recipe(
        0,
        RetainedCaptureRecipe::new(
            1,
            2,
            CaptureJitter::Halton23,
            false,
            vec![CaptureOutput::Srgb8],
            4 * 1024 * 1024,
        )
        .unwrap(),
    )
    .unwrap();
    let packet = std::ptr::from_ref(host.offscreen.as_ref().unwrap());
    let before = authority.submitted_serial();
    let first = renderer
        .execution
        .acquire_capture(
            &renderer.context,
            [64, 64],
            renderer.presentation.output_format(),
        )
        .unwrap()
        .unwrap();
    let second = renderer
        .execution
        .acquire_capture(
            &renderer.context,
            [64, 64],
            renderer.presentation.output_format(),
        )
        .unwrap()
        .unwrap();

    assert!(!renderer.render_retained(&mut host).unwrap());
    assert_eq!(authority.submitted_serial(), before);
    assert!(std::ptr::eq(packet, host.offscreen.as_ref().unwrap()));
    drop((first, second));
    assert!(!renderer.render_retained(&mut host).unwrap());
    assert_eq!(authority.submitted_serial(), before + 1);
    assert!(host.offscreen.is_none());
    renderer.drain().unwrap();
    assert!(is_cleared(&image.bytes(&device)));
}
