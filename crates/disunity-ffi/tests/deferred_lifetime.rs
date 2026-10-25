use worldrunner_disunity_ffi::{abi::*, result_code, with_native_world};

#[test]
fn abi_destruction_invalidates_before_step_and_retires_at_phase_end() {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
        assert_eq!(
            disunity_transform_set_position(world, object, 2.0, 4.0, 6.0).code,
            result_code::OK
        );
        assert_eq!(advance_frame(world, 0.0, 0).code, result_code::OK);
        let native = with_native_world(world, |w| w.snapshot().objects[0].handle).unwrap();
        assert_eq!(
            disunity_gameobject_destroy(world, object).code,
            result_code::OK
        );
        assert_eq!(
            with_native_world(world, |w| w.storage_counts()).unwrap(),
            (1, 1)
        );
        assert!(with_native_world(world, |w| w.object(native).is_err()).unwrap());
        assert_eq!(
            disunity_gameobject_destroy(world, object).code,
            result_code::INVALID_HANDLE
        );
        assert_eq!(advance_frame(world, 0.0, 1).code, result_code::OK);
        assert!(with_native_world(world, |w| w.snapshot().components.is_empty()).unwrap());
        assert_eq!(
            with_native_world(world, |w| w.storage_counts()).unwrap(),
            (0, 0)
        );
        verify_stale_generation(world, object);
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}

fn verify_stale_generation(world: u64, object: u64) {
    unsafe {
        let mut replacement = 0;
        assert_eq!(
            disunity_gameobject_create(world, &mut replacement).code,
            result_code::OK
        );
        assert_ne!(object, replacement);
        assert_eq!(
            disunity_transform_set_position(world, object, 1.0, 1.0, 1.0).code,
            result_code::ERROR
        );
        assert_eq!(advance_frame(world, 0.0, 2).code, result_code::OK);
    }
}

unsafe fn advance_frame(
    world: u64,
    delta: f32,
    _frame: u64,
) -> worldrunner_disunity_ffi::DisunityResult {
    let mut work = worldrunner_disunity_ffi::scheduling::DisunityAdvance::default();
    let elapsed = std::time::Duration::try_from_secs_f32(delta)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok());
    unsafe {
        worldrunner_disunity_ffi::scheduling::disunity_world_advance(
            world,
            elapsed.unwrap_or(0),
            if elapsed.is_some() { 1_000_000_000 } else { 0 },
            &mut work,
        )
    }
}

#[test]
fn parent_destroy_rejects_child_reads_and_exports_no_stale_descendants() {
    use worldrunner_disunity_ffi::{
        DisunityObjectState, hierarchy::*, scheduling::DisunityAdvance,
    };
    unsafe {
        let (mut world, mut parent, mut child) = (0, 0, 0);
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut parent).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut child).code, 0);
        let mut work = DisunityAdvance::default();
        assert_eq!(
            disunity_transform_parent(world, child, parent, 0, &mut work).code,
            0
        );
        assert_eq!(disunity_gameobject_destroy(world, parent).code, 0);
        let mut state = DisunityObjectState {
            object: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rotation_y_radians: 0.0,
            material_slot: 0,
        };
        assert_eq!(
            disunity_gameobject_read(world, child, &mut state).code,
            result_code::INVALID_HANDLE
        );
        let mut written = 99;
        assert_eq!(
            disunity_world_export_objects(world, &mut state, 1, &mut written).code,
            0
        );
        assert_eq!(written, 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
