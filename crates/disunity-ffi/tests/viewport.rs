//! A command buffer's viewport through the ABI: the call forwards the rectangle as one record, and a
//! take hands it back with kind 18 and the bytes that carry it.
use worldrunner_disunity_ffi::{
    abi::*, graphics_commands::*, graphics_context::*, graphics_offscreen::*,
};
use worldrunner_world_simulation::graphics::{OffscreenWork, snapshot_wire::decode_work};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

/// Takes the frame's work and the decoded detail of each record.
fn take(world: u64) -> Vec<(DisunityOffscreenWork, OffscreenWork)> {
    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 4];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 4, &mut count).code,
            0
        );
    }
    records[..count as usize]
        .iter()
        .map(|record| {
            let (mut length, mut bytes) = (0, [0_u8; 256]);
            unsafe {
                assert_eq!(
                    disunity_offscreen_detail(
                        world,
                        record.bindings,
                        bytes.as_mut_ptr(),
                        256,
                        &mut length
                    )
                    .code,
                    0
                );
            }
            (*record, decode_work(&bytes[..length as usize]).unwrap())
        })
        .collect()
}

#[test]
fn the_rectangle_reaches_the_frame_as_one_record_of_kind_eighteen() {
    let world = new_world();
    let code = disunity_offscreen_set_viewport(world, -2.0, 3.5, 640.0, 360.25).code;
    assert_eq!(code, 0);
    let taken = take(world);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].0.kind, KIND_SET_VIEWPORT);
    assert_eq!(KIND_SET_VIEWPORT, 18);
    assert_eq!(
        taken[0].1,
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
    let rects: Vec<_> = take(world).into_iter().map(|(_, work)| work).collect();
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
    assert!(take(world).is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
