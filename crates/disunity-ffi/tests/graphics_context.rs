//! Scriptable render context records through the ABI: each call validates and forwards one
//! record, and the frame's work holds the records.
#[path = "common/frame.rs"]
mod frame;

use frame::snapshot;
use worldrunner_disunity_ffi::{abi::*, graphics_commands::*, graphics_context::*, sprites::*};
use worldrunner_world_simulation::graphics::{OffscreenWork, TargetRef};

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
    let taken = snapshot(world).work;
    assert_eq!(taken.len(), 2);
    let OffscreenWork::Cull {
        id,
        culling_mask,
        planes: got,
        options,
        ..
    } = &taken[0]
    else {
        panic!("expected a culling result, got {:?}", taken[0]);
    };
    assert_eq!((*id, *culling_mask, *options), (5, 0xF0F0_0001, 6));
    assert_eq!(got.concat(), planes);
    let OffscreenWork::Cull { id, planes, .. } = &taken[1] else {
        panic!("expected a culling result");
    };
    assert_eq!((*id, planes.len()), (6, 0));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

fn target(kind: u32, name_id: i32, render: u64) -> DisunityTargetRef {
    DisunityTargetRef {
        kind,
        name_id,
        render,
    }
}

/// Calls the setup with the view and projection side by side as the ABI takes them; a missing one
/// passes no matrices.
unsafe fn setup(
    world: u64,
    camera: u64,
    view: Option<&[f32; 16]>,
    projection: Option<&[f32; 16]>,
    target: *const DisunityTargetRef,
) -> DisunityResult {
    let joined: Option<Vec<f32>> = view
        .zip(projection)
        .map(|(view, projection)| view.iter().chain(projection).copied().collect());
    let matrices = joined
        .as_ref()
        .map_or(std::ptr::null(), |both| both.as_ptr());
    unsafe {
        disunity_offscreen_setup_camera(world, camera, matrices, target, std::ptr::null(), 0, 0)
    }
}

#[test]
fn a_camera_setup_is_forwarded_with_its_matrices_and_target() {
    let world = new_world();
    let camera = camera(world);
    let view: [f32; 16] = std::array::from_fn(|i| i as f32 - 3.5);
    let projection: [f32; 16] = std::array::from_fn(|i| 0.25 * i as f32);
    unsafe {
        assert_eq!(
            setup(
                world,
                camera,
                Some(&view),
                Some(&projection),
                &target(1, -9, 0)
            )
            .code,
            0
        );
        assert_eq!(
            setup(
                world,
                camera,
                Some(&view),
                Some(&projection),
                &target(2, 0, 0)
            )
            .code,
            0
        );
    }
    let taken = snapshot(world).work;
    assert_eq!(taken.len(), 2);
    let OffscreenWork::SetupCamera {
        view: got_view,
        projection: got_projection,
        target: got_target,
        ..
    } = &taken[0]
    else {
        panic!("expected a camera setup, got {:?}", taken[0]);
    };
    assert_eq!((got_view, got_projection), (&view, &projection));
    assert_eq!(*got_target, TargetRef::Temporary(-9));
    assert!(matches!(
        taken[1],
        OffscreenWork::SetupCamera {
            target: TargetRef::CameraTarget,
            ..
        }
    ));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_camera_setup_with_bad_arguments_is_refused_and_records_nothing() {
    let world = new_world();
    let camera = camera(world);
    let good = [0.0_f32; 16];
    let mut nan = good;
    nan[3] = f32::NAN;
    let none = target(3, 0, 0);
    unsafe {
        rejected(setup(world, camera, None, Some(&good), &none).code);
        rejected(setup(world, camera, Some(&good), None, &none).code);
        rejected(setup(world, camera, Some(&good), Some(&good), std::ptr::null()).code);
        rejected(setup(world, camera + 999, Some(&good), Some(&good), &none).code);
        rejected(setup(world, camera, Some(&nan), Some(&good), &none).code);
        rejected(setup(world, camera, Some(&good), Some(&good), &target(9, 0, 0)).code);
        rejected(setup(world, camera, Some(&good), Some(&good), &target(0, 0, 77)).code);
    }
    assert_eq!(snapshot(world).work.len(), 0);
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
    assert_eq!(snapshot(world).work.len(), 1);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
