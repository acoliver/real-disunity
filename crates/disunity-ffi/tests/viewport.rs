//! A command buffer's viewport through the ABI: the call forwards the rectangle as one record, and the
//! frame's work holds it.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{abi::*, graphics_context::*};
use worldrunner_world_simulation::graphics::OffscreenWork;

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

#[test]
fn the_rectangle_reaches_the_frame_as_one_record() {
    let world = new_world();
    let code = disunity_offscreen_set_viewport(world, -2.0, 3.5, 640.0, 360.25).code;
    assert_eq!(code, 0);
    let taken = snapshot(world).work;
    assert_eq!(taken.len(), 1);
    assert_eq!(
        taken[0],
        OffscreenWork::SetViewport {
            rect: [-2.0, 3.5, 640.0, 360.25]
        }
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn viewports_are_taken_back_in_call_order() {
    let world = new_world();
    assert_eq!(
        disunity_offscreen_set_viewport(world, 0.0, 0.0, 1.0, 2.0).code,
        0
    );
    assert_eq!(
        disunity_offscreen_set_viewport(world, 4.0, 5.0, 0.0, 0.0).code,
        0
    );
    let rects = snapshot(world).work;
    assert_eq!(
        rects,
        [
            OffscreenWork::SetViewport {
                rect: [0.0, 0.0, 1.0, 2.0]
            },
            OffscreenWork::SetViewport {
                rect: [4.0, 5.0, 0.0, 0.0]
            },
        ]
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_value_that_is_not_finite_or_a_negative_size_is_refused_and_records_nothing() {
    let world = new_world();
    let (nan, inf) = (f32::NAN, f32::INFINITY);
    for (x, y, width, height) in [
        (nan, 0.0, 1.0, 1.0),
        (0.0, inf, 1.0, 1.0),
        (0.0, 0.0, nan, 1.0),
        (0.0, 0.0, 1.0, f32::NEG_INFINITY),
        (0.0, 0.0, -1.0, 1.0),
        (0.0, 0.0, 1.0, -0.5),
    ] {
        assert_ne!(
            disunity_offscreen_set_viewport(world, x, y, width, height).code,
            0
        );
    }
    assert_ne!(
        disunity_offscreen_set_viewport(world + 999, 0.0, 0.0, 1.0, 1.0).code,
        0,
        "an unknown world"
    );
    assert!(snapshot(world).work.is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
