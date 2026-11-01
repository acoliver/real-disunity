use worldrunner_disunity_ffi::{abi::*, camera_layers::*};

const ANIMATOR: u32 = 95;

fn animator() -> (u64, u64) {
    let (mut world, mut object, mut animator) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_camera_layer_create(world, object, ANIMATOR, &mut animator).code,
            0
        );
    }
    (world, animator)
}

fn enabled(world: u64, animator: u64) -> u32 {
    let mut value = 7;
    unsafe {
        assert_eq!(
            disunity_camera_layer_read(world, animator, &mut value).code,
            0
        );
    }
    value
}

#[test]
fn an_animator_is_created_enabled_and_keeps_the_flag_it_is_given() {
    let (world, animator) = animator();
    assert_eq!(enabled(world, animator), 1);
    assert_eq!(disunity_camera_layer_write(world, animator, 0).code, 0);
    assert_eq!(enabled(world, animator), 0);
    assert_eq!(disunity_camera_layer_write(world, animator, 1).code, 0);
    assert_eq!(enabled(world, animator), 1);
}

#[test]
fn a_flag_other_than_zero_or_one_is_rejected_and_changes_nothing() {
    let (world, animator) = animator();
    assert_ne!(disunity_camera_layer_write(world, animator, 2).code, 0);
    assert_eq!(enabled(world, animator), 1);
}
