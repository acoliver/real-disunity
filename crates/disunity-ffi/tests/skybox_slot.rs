use worldrunner_disunity_ffi::{abi::*, camera_layers::*};

const SKYBOX: u32 = 45;

fn skybox() -> (u64, u64) {
    let (mut world, mut object, mut skybox) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_camera_layer_create(world, object, SKYBOX, &mut skybox).code,
            0
        );
    }
    (world, skybox)
}

fn enabled(world: u64, skybox: u64) -> u32 {
    let mut value = 7;
    unsafe {
        assert_eq!(
            disunity_camera_layer_read(world, skybox, &mut value).code,
            0
        );
    }
    value
}

#[test]
fn a_skybox_is_created_enabled_and_keeps_the_flag_it_is_given() {
    let (world, skybox) = skybox();
    assert_eq!(enabled(world, skybox), 1);
    assert_eq!(disunity_camera_layer_write(world, skybox, 0).code, 0);
    assert_eq!(enabled(world, skybox), 0);
    assert_eq!(disunity_camera_layer_write(world, skybox, 1).code, 0);
    assert_eq!(enabled(world, skybox), 1);
}

#[test]
fn a_skybox_flag_other_than_zero_or_one_is_rejected_and_changes_nothing() {
    let (world, skybox) = skybox();
    assert_ne!(disunity_camera_layer_write(world, skybox, 2).code, 0);
    assert_eq!(enabled(world, skybox), 1);
}
