//! What a flush reports of the render textures a dispatch reads: their non-zero texels and value
//! ranges after the operations before the dispatch and right before it, and the cameras that drew
//! them. Every expected value comes from the camera's tint and clear color.

use super::flush::{HEIGHT, camera_render, host, scaling};
use super::gpu::{Quad, SIDE, target};
use super::kernels::{copy_object, write_compute};
use super::support::{compute_buffer, dispatch, snapshot, world};

/// The camera draws 0.5 over the left half of the 4x4 map and clears the rest to 0.25.
const DRAWN: &str = "16 texels, 16 non-zero, values [0.25, 0.5]";
/// A map nothing has drawn into holds zeros.
const EMPTY: &str = "16 texels, 0 non-zero, values [0, 0]";

#[test]
fn a_dispatch_reports_the_height_map_the_camera_drew_after_the_pass_and_before_the_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    let texture = &outcome.report.traces[0].textures[0];
    assert_eq!(texture.after_operations.as_deref(), Some(DRAWN));
    assert_eq!(texture.before_dispatch.as_deref(), Some(DRAWN));
    assert_eq!(texture.camera_notes.len(), 1, "{:?}", texture.camera_notes);
    let note = &texture.camera_notes[0];
    assert!(note.contains("drew 1"), "{note}");
    assert!(note.contains("R16Float"), "{note}");
}

#[test]
fn a_dispatch_before_the_camera_reports_a_map_of_zeros_and_the_one_after_it_the_drawn_map() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let shader = scaling(&mut world);
    let (before, after) = (
        compute_buffer(&mut world, SIDE * SIDE),
        compute_buffer(&mut world, SIDE * SIDE),
    );
    dispatch(&mut world, shader, (heights, before), [2, 2, 1]);
    camera_render(&mut world, heights);
    dispatch(&mut world, shader, (heights, after), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    let reads: Vec<_> = outcome
        .report
        .traces
        .iter()
        .map(|trace| {
            let texture = &trace.textures[0];
            (
                texture.after_operations.clone().unwrap(),
                texture.before_dispatch.clone().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        reads,
        [
            (EMPTY.to_owned(), EMPTY.to_owned()),
            (DRAWN.to_owned(), DRAWN.to_owned())
        ]
    );
}
