//! What a flush reports of the render textures a dispatch reads: their non-zero texels and value
//! ranges after the operations before the dispatch and right before it, and the cameras that drew
//! them. Every expected value comes from the camera's tint and clear color.

use super::super::frame::TEST_VARIANTS;
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
    // The draw's note, the render's setup and one program.
    assert_eq!(texture.camera_notes.len(), 3, "{:?}", texture.camera_notes);
    let note = &texture.camera_notes[0];
    assert!(note.contains("drew 1"), "{note}");
    assert!(note.contains("R16Float"), "{note}");
    assert_eq!(
        texture.camera_notes[1],
        "setup: viewport [4, 4], color clear [0.25, 0.0, 1.0, 1.0] (Some(R16Float)), depth no \
         depth (None)"
    );
    assert!(texture.camera_notes[2].contains("1 draws, 4 vertices"));
}

#[test]
fn the_test_render_shows_the_texels_a_fragment_reached_by_their_difference_from_its_clear() {
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
    // The camera draws over the left half of the 4x4 map: 8 of its 16 texels.
    assert_eq!(
        outcome.report.traces[0].textures[0].test_render.as_deref(),
        Some("8 of 16 texels differ from the clear ([0.3125, 0.6875, 0.4375, 0.8125])")
    );
    // The test render is not one of the frame's camera renders.
    assert_eq!(outcome.report.camera_draws.len(), 1);
    assert_eq!(outcome.report.camera_passes.len(), 1);
}

#[test]
fn the_changed_test_renders_are_reported_once_each_by_what_they_change_with_the_texels_they_write()
{
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
    // The camera's shader culls nothing, blends nothing and tests no depth, so every change
    // leaves the 8 drawn texels as they are, whatever the fragment stage writes.
    let drawn = "8 of 16 texels differ from the clear ([0.3125, 0.6875, 0.4375, 0.8125])";
    let renders = &outcome.report.traces[0].textures[0].variant_renders;
    let expected: Vec<String> = TEST_VARIANTS
        .iter()
        .map(|(what, _)| format!("{what}: {drawn}"))
        .collect();
    assert_eq!(renders, &expected);
    assert_eq!(outcome.report.camera_draws.len(), 1);
    // Reported once: the variants are made for the first probed work only.
    let again = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        again.report.traces[0].textures[0]
            .variant_renders
            .is_empty()
    );
    assert_eq!(
        again.report.traces[0].textures[0].test_render.as_deref(),
        Some(drawn)
    );
}

#[test]
fn a_camera_render_into_a_texture_no_dispatch_reads_is_not_examined() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let other = target(&mut world, HEIGHT);
    camera_render(&mut world, other);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert_eq!(outcome.report.camera_passes.len(), 1);
    assert!(outcome.report.camera_passes[0].programs.is_empty());
    assert_eq!(outcome.report.traces[0].textures[0].test_render, None);
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
