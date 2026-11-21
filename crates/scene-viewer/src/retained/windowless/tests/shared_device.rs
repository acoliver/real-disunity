//! A windowless session on a device made before it runs a caller's offscreen work in its frame.
use super::{Driver, host};
use crate::SharedGpu;
use crate::renderer::retained_tests::offscreen::{clearing_operations, is_cleared};
use worldrunner_render_gpu::device::DeviceRequirements;

#[test]
fn a_windowless_session_on_a_shared_device_runs_the_callers_offscreen_work_in_its_frame() {
    let mut gpu = SharedGpu::headless_for_test();
    let (device, authority) = gpu
        .device(&crate::tests::options(), &DeviceRequirements::default())
        .unwrap();
    let (operations, image, _runtime) = clearing_operations(&device, &authority);
    let mut driver = Driver::default();
    let mut host = host(crate::renderer::retained_tests::source());
    host.admit_offscreen(operations);
    let mut run = host.windowless(&mut driver, Some(gpu)).unwrap();

    let before = authority.submitted_serial();
    assert!(run.frame().unwrap());
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "the frame and the caller's work are one submission on the caller's authority"
    );
    run.shutdown().unwrap();

    assert!(
        is_cleared(&image.bytes(&device)),
        "the camera cleared the render texture in the session's frame"
    );
}
