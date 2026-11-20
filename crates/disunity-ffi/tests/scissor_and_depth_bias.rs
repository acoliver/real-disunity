//! A command buffer's scissor rectangle and global depth bias through the ABI: each call forwards
//! one record, and a take hands it back with kinds 20 to 22 and the bytes that carry it.
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
fn the_scissor_and_the_bias_reach_the_frame_as_records_of_kinds_twenty_to_twenty_two() {
    let world = new_world();
    assert_eq!(
        disunity_offscreen_enable_scissor(world, 4.0, 5.5, 56.0, 40.25).code,
        0
    );
    assert_eq!(disunity_offscreen_set_depth_bias(world, 1.0, 2.5).code, 0);
    assert_eq!(disunity_offscreen_disable_scissor(world).code, 0);
    let taken = take(world);
    let kinds: Vec<_> = taken.iter().map(|(record, _)| record.kind).collect();
    assert_eq!(
        kinds,
        [
            KIND_ENABLE_SCISSOR,
            KIND_SET_DEPTH_BIAS,
            KIND_DISABLE_SCISSOR
        ]
    );
    assert_eq!(
        [
            KIND_ENABLE_SCISSOR,
            KIND_DISABLE_SCISSOR,
            KIND_SET_DEPTH_BIAS
        ],
        [20, 21, 22]
    );
    let work: Vec<_> = taken.into_iter().map(|(_, work)| work).collect();
    assert_eq!(
        work,
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
    assert!(take(world).is_empty());
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
