//! Scriptable render context records through the ABI: each call validates and forwards one
//! record, and a take hands the records back with the bytes that carry their details.
use worldrunner_disunity_ffi::{
    abi::*, graphics_commands::*, graphics_context::*, graphics_offscreen::*, sprites::*,
};
use worldrunner_world_simulation::graphics::{OffscreenWork, snapshot_wire::decode_work};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn camera(world: u64) -> u64 {
    let (mut object, mut camera) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
    }
    camera
}

fn rejected(code: i32) {
    assert_ne!(code, 0);
}

/// Takes the frame's work and the decoded detail of each context record.
fn take(world: u64) -> Vec<(DisunityOffscreenWork, OffscreenWork)> {
    let mut count = 0;
    let mut records = [DisunityOffscreenWork::default(); 8];
    unsafe {
        assert_eq!(
            disunity_offscreen_take(world, records.as_mut_ptr(), 8, &mut count).code,
            0
        );
    }
    records[..count as usize]
        .iter()
        .map(|record| {
            let mut length = 0;
            let mut bytes = [0_u8; 512];
            unsafe {
                assert_eq!(
                    disunity_offscreen_detail(
                        world,
                        record.bindings,
                        bytes.as_mut_ptr(),
                        512,
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
fn a_culling_result_is_forwarded_with_its_planes() {
    let world = new_world();
    let camera = camera(world);
    let planes: [f32; 12] = std::array::from_fn(|i| 0.5 * i as f32 - 2.0);
    unsafe {
        assert_eq!(
            disunity_offscreen_cull(world, 5, camera, 0xF0F0_0001, planes.as_ptr(), 3, 6).code,
            0
        );
        assert_eq!(
            disunity_offscreen_cull(world, 6, camera, 1, std::ptr::null(), 0, 0).code,
            0
        );
    }
    let taken = take(world);
    assert_eq!(taken.len(), 2);
    assert_eq!(taken[0].0.kind, KIND_CULL);
    assert_ne!(taken[0].0.bindings, 0);
    let OffscreenWork::Cull {
        id,
        culling_mask,
        planes: got,
        options,
        ..
    } = &taken[0].1
    else {
        panic!("expected a culling result, got {:?}", taken[0].1);
    };
    assert_eq!((*id, *culling_mask, *options), (5, 0xF0F0_0001, 6));
    assert_eq!(got.concat(), planes);
    let OffscreenWork::Cull { id, planes, .. } = &taken[1].1 else {
        panic!("expected a culling result");
    };
    assert_eq!((*id, planes.len()), (6, 0));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_culling_result_with_bad_arguments_is_refused_and_records_nothing() {
    let world = new_world();
    let camera = camera(world);
    let planes = [0.0_f32; 44];
    unsafe {
        rejected(disunity_offscreen_cull(world, 1, camera, 1, std::ptr::null(), 1, 0).code);
        rejected(disunity_offscreen_cull(world, 1, camera, 1, planes.as_ptr(), 11, 0).code);
        rejected(disunity_offscreen_cull(world, 0, camera, 1, planes.as_ptr(), 1, 0).code);
        rejected(disunity_offscreen_cull(world, 1, camera + 999, 1, planes.as_ptr(), 1, 0).code);
        let mut nan = [0.0_f32; 4];
        nan[2] = f32::NAN;
        rejected(disunity_offscreen_cull(world, 1, camera, 1, nan.as_ptr(), 1, 0).code);
        assert_eq!(
            disunity_offscreen_cull(world, 2, camera, 1, planes.as_ptr(), 1, 0).code,
            0
        );
        rejected(disunity_offscreen_cull(world, 2, camera, 1, planes.as_ptr(), 1, 0).code);
    }
    assert_eq!(take(world).len(), 1);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
