//! A flush that plans more draws than the host's budget builds and submits its records in parts,
//! so the planned draws of one part are dropped before the next is planned. The parts draw what
//! one build of the whole run draws, in the order the records were called.
//!
//! Every camera of these worlds draws the same three renderers over the left half of its
//! target. A flush renders each camera twice, into its target and into a scratch copy that tells
//! which texels the camera wrote, and the first flush of a host adds a render for each state
//! variant it tests. The tests that count builds flush once to get those out of the way.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{ComputeBufferId, GraphicsSnapshot};

use super::super::flush::{FlushOutcome, OffscreenHost};
use super::flush::{HEIGHT, camera_render, camera_renders, expected, floats, host, scaling};
use super::gpu::{Quad, SIDE, target};
use super::kernels::{copy_object, write_compute};
use super::support::{compute_buffer, dispatch, snapshot, world};

/// The draws one render of a camera plans in these worlds.
const PER_RENDER: usize = 3;
/// The renders of one camera once the host has made its state variants.
const RENDERS: usize = 2;
const CAMERAS: usize = 3;

/// A flush of `snapshot` after the one that makes the host's state variants.
fn flush_after_warm_up(
    host: &mut OffscreenHost,
    snapshot: &GraphicsSnapshot,
    budget: usize,
) -> FlushOutcome {
    host.flush(snapshot, &Quad).unwrap();
    host.set_draw_budget(budget);
    let outcome = host.flush(snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    outcome
}

/// Three camera renders into three height maps, each read into a buffer by a dispatch after all
/// the renders, flushed by a host with room for `budget` planned draws in one build.
fn three_cameras(budget: usize) -> (FlushOutcome, World, Vec<ComputeBufferId>) {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let maps: Vec<_> = (0..CAMERAS).map(|_| target(&mut world, HEIGHT)).collect();
    camera_renders(&mut world, &maps);
    let shader = scaling(&mut world);
    let buffers: Vec<_> = maps
        .iter()
        .map(|map| {
            let buffer = compute_buffer(&mut world, SIDE * SIDE);
            dispatch(&mut world, shader, (*map, buffer), [2, 2, 1]);
            buffer
        })
        .collect();
    let snapshot = snapshot(&mut world);
    let outcome = flush_after_warm_up(&mut host, &snapshot, budget);
    outcome.apply(&mut world).unwrap();
    (outcome, world, buffers)
}

fn every_map_is_drawn(world: &World, buffers: &[ComputeBufferId]) {
    for buffer in buffers {
        assert_eq!(floats(world, *buffer), expected(), "{buffer:?}");
    }
}

#[test]
fn a_flush_within_its_budget_is_one_build_holding_every_draw() {
    let (outcome, world, buffers) = three_cameras(usize::MAX);
    every_map_is_drawn(&world, &buffers);
    assert_eq!(outcome.report.phases.builds, 1);
    assert_eq!(
        outcome.report.phases.draws_held_most,
        CAMERAS * RENDERS * PER_RENDER
    );
}

#[test]
fn a_flush_over_its_budget_is_built_in_parts_that_draw_the_same_pixels() {
    let budget = 2 * RENDERS * PER_RENDER;
    let (outcome, world, buffers) = three_cameras(budget);
    every_map_is_drawn(&world, &buffers);
    // Two cameras' renders fill the budget, and the third camera's start the next part.
    assert_eq!(outcome.report.phases.builds, 2);
    assert_eq!(outcome.report.phases.draws_held_most, budget);
}

#[test]
fn no_build_holds_more_draws_than_the_budget_unless_one_record_plans_more_alone() {
    let (outcome, world, buffers) = three_cameras(RENDERS * PER_RENDER + 1);
    every_map_is_drawn(&world, &buffers);
    assert_eq!(outcome.report.phases.builds, CAMERAS);
    assert_eq!(outcome.report.phases.draws_held_most, RENDERS * PER_RENDER);
    let (alone, world, buffers) = three_cameras(1);
    every_map_is_drawn(&world, &buffers);
    assert_eq!(alone.report.phases.builds, CAMERAS * RENDERS);
    assert_eq!(
        alone.report.phases.draws_held_most, PER_RENDER,
        "a render that plans more than the budget is built alone"
    );
}

/// Dispatches that read a height map before and after two camera renders into it, flushed by a
/// fresh host with room for `budget` planned draws in one build. The buffers the dispatches
/// filled, and the builds the flush made.
fn around_a_dispatch(budget: usize) -> (Vec<f32>, Vec<f32>, usize) {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    host.set_draw_budget(budget);
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let shader = scaling(&mut world);
    let before = compute_buffer(&mut world, SIDE * SIDE);
    let after = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, before), [2, 2, 1]);
    camera_render(&mut world, heights);
    camera_render(&mut world, heights);
    dispatch(&mut world, shader, (heights, after), [2, 2, 1]);
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(outcome.report.failures.is_empty());
    outcome.apply(&mut world).unwrap();
    (
        floats(&world, before),
        floats(&world, after),
        outcome.report.phases.builds,
    )
}

#[test]
fn parts_keep_the_order_of_the_records_around_a_dispatch() {
    let (before, after, whole) = around_a_dispatch(usize::MAX);
    assert_eq!(before, vec![0.0; 16], "read before any render");
    assert_eq!(after, expected());
    let (split_before, split_after, parts) = around_a_dispatch(1);
    assert!(parts > whole, "{parts} builds against {whole}");
    assert_eq!(split_before, before);
    assert_eq!(split_after, after);
}
