use worldrunner_disunity_ffi::{abi::*, command_kind, result_code, with_native_world};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
};

#[test]
fn abi_and_rust_observe_one_store_and_identical_results() {
    let mut handle = 0;
    assert_eq!(
        unsafe { disunity_world_create(&mut handle) }.code,
        result_code::OK
    );
    let mut object = 0;
    assert_eq!(
        unsafe { disunity_gameobject_create(handle, &mut object) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { disunity_transform_set_position(handle, object, 2.0, 3.0, 4.0) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { disunity_world_step(handle, 0.0, 1) }.code,
        result_code::OK
    );
    let snapshot = with_native_world(handle, |world| world.snapshot()).unwrap();
    let mut native = World::new(WorldSettings::default()).unwrap();
    let direct = native.create_object(ObjectDesc::default()).unwrap();
    let value = LocalTransform {
        translation: [2.0, 3.0, 4.0].into(),
        ..Default::default()
    };
    native.set_local_transform(direct, value).unwrap();
    assert_eq!(
        snapshot.objects[0].local_transform,
        native.object(direct).unwrap().local_transform
    );
    assert_eq!(snapshot.objects.len(), 1);
    assert_eq!(snapshot.components.len(), 1);
    compare_renderer(handle, object, &mut native, direct);
    assert_eq!(
        unsafe { disunity_gameobject_destroy(handle, object) }.code,
        result_code::OK
    );
    assert!(with_native_world(handle, |w| w.snapshot().objects.is_empty()).unwrap());
    assert!(with_native_world(handle, |w| w.snapshot().components.is_empty()).unwrap());
    assert_eq!(
        unsafe { disunity_world_destroy(handle) }.code,
        result_code::OK
    );
    assert!(with_native_world(handle, |w| w.snapshot()).is_err());
}

fn compare_renderer(
    handle: u64,
    object: u64,
    native: &mut World,
    direct: worldrunner_world_simulation::ObjectHandle,
) {
    let command = DisunityCommand {
        object,
        kind: command_kind::SET_MATERIAL_SLOT,
        x: 9.0,
        y: 0.0,
        z: 0.0,
    };
    assert_eq!(
        unsafe { disunity_command_buffer_submit(handle, &command, 1) }.code,
        result_code::OK
    );
    assert_eq!(
        unsafe { disunity_world_step(handle, 0.0, 2) }.code,
        result_code::OK
    );
    let mut states = [worldrunner_disunity_ffi::DisunityObjectState {
        object: 0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        rotation_y_radians: 0.0,
        material_slot: 0,
    }];
    let mut written = 0;
    assert_eq!(
        unsafe { disunity_world_export_objects(handle, states.as_mut_ptr(), 1, &mut written) }.code,
        result_code::OK
    );
    assert_eq!(written, 1);
    assert_eq!(states[0].material_slot, 9);
    let direct_component = native
        .add_component(direct, ComponentDesc::Renderer { material_slot: 9 })
        .unwrap();
    assert_eq!(
        with_native_world(handle, |w| w.snapshot().components[1].desc.clone()).unwrap(),
        native.component(direct_component).unwrap().desc
    );
}

#[test]
fn wrong_world_and_unloaded_handles_never_alias() {
    let (mut a, mut b, mut first, mut second) = (0, 0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut a).code, result_code::OK);
        assert_eq!(disunity_world_create(&mut b).code, result_code::OK);
        disunity_gameobject_create(a, &mut first);
        disunity_gameobject_create(b, &mut second);
        assert_ne!(first, second);
        assert_eq!(
            disunity_gameobject_destroy(b, first).code,
            result_code::INVALID_HANDLE
        );
        disunity_world_destroy(a);
        disunity_world_create(&mut a);
        disunity_gameobject_create(a, &mut second);
        assert_eq!(
            disunity_gameobject_destroy(a, first).code,
            result_code::INVALID_HANDLE
        );
        disunity_world_destroy(a);
        disunity_world_destroy(b);
    }
}

#[test]
fn failed_command_batch_does_not_apply_its_valid_prefix() {
    let (mut world, mut object) = (0, 0);
    unsafe {
        disunity_world_create(&mut world);
        disunity_gameobject_create(world, &mut object);
        let commands = [
            DisunityCommand {
                object,
                kind: command_kind::SET_POSITION,
                x: 5.0,
                y: 0.0,
                z: 0.0,
            },
            DisunityCommand {
                object,
                kind: 999,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        ];
        disunity_command_buffer_submit(world, commands.as_ptr(), 2);
        assert_eq!(disunity_world_step(world, 0.0, 1).code, result_code::ERROR);
        let snapshot = with_native_world(world, |w| w.snapshot()).unwrap();
        assert_eq!(
            snapshot.objects[0].local_transform,
            LocalTransform::default()
        );
        disunity_world_destroy(world);
    }
}
