//! A frame the state refuses for a reason no record or render texture causes is refused at once:
//! the host does not build the parts one at a time to look for a culprit.

use super::super::flush::FlushError;
use super::flush::{HEIGHT, camera_render, host, scaling};
use super::gpu::{Quad, SIDE, target};
use super::kernels::{copy_object, write_compute};
use super::support::{compute_buffer, dispatch, snapshot, world};

#[test]
fn constants_no_buffer_can_hold_refuse_the_frame_at_once_without_building_its_parts() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    // A buffer of 8 bytes holds none of the 16-byte blocks the camera's draw packs.
    host.set_constants_limit(8);
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
