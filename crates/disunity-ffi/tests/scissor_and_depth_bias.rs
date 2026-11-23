//! A command buffer's scissor rectangle and global depth bias through the ABI: each call forwards
//! one record, and the frame's work holds them in call order.
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
fn the_scissor_and_the_bias_reach_the_frame_as_records_in_call_order() {
    let world = new_world();
    assert_eq!(
        disunity_offscreen_enable_scissor(world, 4.0, 5.5, 56.0, 40.25).code,
        0
    );
    assert_eq!(disunity_offscreen_set_depth_bias(world, 1.0, 2.5).code, 0);
    assert_eq!(disunity_offscreen_disable_scissor(world).code, 0);
    assert_eq!(
        snapshot(world).work,
        [
            OffscreenWork::EnableScissor {
                rect: [4.0, 5.5, 56.0, 40.25]
            },
            OffscreenWork::SetDepthBias {
                bias: 1.0,
                slope_bias: 2.5
            },
            OffscreenWork::DisableScissor,
        ]
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_scissor_or_bias_that_is_not_valid_is_refused_and_records_nothing() {
    let world = new_world();
    let (nan, inf) = (f32::NAN, f32::INFINITY);
    for (x, y, width, height) in [
        (nan, 0.0, 1.0, 1.0),
        (0.0, inf, 1.0, 1.0),
        (0.0, 0.0, -1.0, 1.0),
        (0.0, 0.0, 1.0, -0.5),
    ] {
        assert_ne!(
            disunity_offscreen_enable_scissor(world, x, y, width, height).code,
            0
        );
    }
    assert_ne!(disunity_offscreen_set_depth_bias(world, nan, 0.0).code, 0);
    assert_ne!(disunity_offscreen_set_depth_bias(world, 0.0, inf).code, 0);
    assert_ne!(disunity_offscreen_disable_scissor(world + 999).code, 0);
    assert!(snapshot(world).work.is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
