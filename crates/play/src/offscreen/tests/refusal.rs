//! A frame the state refuses for a reason no record or render texture causes is refused at once:
//! the host does not build the parts one at a time to look for a culprit.

use super::super::flush::{FlushError, OffscreenHost};
use super::flush::{HEIGHT, camera_render, host, scaling};
use super::gpu::{Quad, SIDE, target};
use super::kernels::{copy_object, write_compute};
use super::support::{compute_buffer, dispatch, snapshot, world};

fn refused_at_once(limits: impl FnOnce(&mut OffscreenHost)) {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    limits(&mut host);
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let other = target(&mut world, HEIGHT);
    camera_render(&mut world, other);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let Err(error) = host.flush(&snapshot, &Quad) else {
        panic!("a block larger than any buffer cannot be drawn");
    };
    assert!(
        matches!(&error, FlushError::Offscreen(error) if error.is_frame_wide()),
        "{error}"
    );
    assert!(error.to_string().contains("no split"), "{error}");
    let traces = host.take_traces();
    let trace = traces.last().unwrap();
    assert_eq!(trace.error.as_deref(), Some(error.to_string().as_str()));
    assert_eq!(
        (trace.phases.refusals, trace.phases.probes),
        (1, 0),
        "{trace:?}"
    );
}

#[test]
fn constants_no_buffer_can_hold_refuse_the_frame_at_once_without_building_its_parts() {
    // A buffer of 8 bytes holds none of the 16-byte blocks the camera's draw packs.
    refused_at_once(|host| host.set_constants_limit(8));
}

#[test]
fn constants_past_what_the_runtime_may_hold_refuse_the_frame_at_once_too() {
    // Each draw packs two 16-byte blocks that differ, so 20 bytes cannot hold them.
    refused_at_once(|host| host.set_constants_total_limit(20));
}
