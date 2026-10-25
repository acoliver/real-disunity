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
        assert_eq!(disunity_world_step(world, 0.0, 0).code, result_code::OK);
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
        assert_eq!(disunity_world_step(world, 0.0, 1).code, result_code::OK);
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
            result_code::OK
        );
        assert_eq!(disunity_world_step(world, 0.0, 2).code, result_code::ERROR);
    }
}
