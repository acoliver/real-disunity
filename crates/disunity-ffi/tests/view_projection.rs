//! A command buffer's view and projection change through the ABI: the call forwards both matrices
//! as one record, and the frame's work holds it.
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
fn both_matrices_reach_the_frame_as_one_record() {
    let world = new_world();
    let view: [f32; 16] = std::array::from_fn(|i| 1.0 + i as f32);
    let projection: [f32; 16] = std::array::from_fn(|i| -0.5 * i as f32);
    let code = unsafe {
        disunity_offscreen_set_view_projection(world, view.as_ptr(), projection.as_ptr())
    }
    .code;
    assert_eq!(code, 0);
    let taken = snapshot(world).work;
    assert_eq!(taken.len(), 1);
    assert_eq!(
        taken[0],
        OffscreenWork::SetViewProjection { view, projection }
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_missing_or_non_finite_matrix_is_refused_and_records_nothing() {
    let world = new_world();
    let good = [0.0_f32; 16];
    let mut nan = good;
    nan[7] = f32::NAN;
    unsafe {
        for (view, projection) in [
            (std::ptr::null(), good.as_ptr()),
            (good.as_ptr(), std::ptr::null()),
            (nan.as_ptr(), good.as_ptr()),
            (good.as_ptr(), nan.as_ptr()),
        ] {
            assert_ne!(
                disunity_offscreen_set_view_projection(world, view, projection).code,
                0
            );
        }
        assert_ne!(
            disunity_offscreen_set_view_projection(world + 999, good.as_ptr(), good.as_ptr()).code,
            0,
            "an unknown world"
        );
    }
    assert!(snapshot(world).work.is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
