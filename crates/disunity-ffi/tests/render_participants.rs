use worldrunner_disunity_ffi::{abi::*, camera_data::*, camera_layers::*, sprites::*};

fn world_object() -> (u64, u64) {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
    }
    (world, object)
}
#[test]
fn unsupported_camera_values_are_typed_and_do_not_publish() {
    let (world, object) = world_object();
    let mut camera = 0;
    unsafe {
        assert_eq!(
            disunity_camera_layer_create(world, object, 115, &mut camera).code,
            0
        );
    }
    let rejected = disunity_camera_data_write(world, camera, 0, 1);
    assert_eq!(
        rejected.detail,
        DisunityErrorReason::InvalidComponent as u64
    );
    let mut value = 99;
    unsafe {
        assert_eq!(
            disunity_camera_data_read(world, camera, 0, &mut value).code,
            0
        );
    }
    assert_eq!(value, 0);
    assert_eq!(
        disunity_camera_data_write(world, camera, 11, 2).detail,
        DisunityErrorReason::InvalidComponent as u64
    );
    assert_eq!(
        disunity_sprite_write_wrong_kind(world, camera).detail,
        DisunityErrorReason::InvalidComponent as u64
    );
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
fn disunity_sprite_write_wrong_kind(world: u64, component: u64) -> DisunityResult {
    let sprite = DisunitySprite {
        enabled: 1,
        ..Default::default()
    };
    unsafe { disunity_sprite_write(world, component, &sprite) }
}
#[test]
fn light_color_and_target_layers_are_atomic_and_bounded() {
    let (world, object) = world_object();
    let mut light = 0;
    unsafe {
        assert_eq!(
            disunity_camera_layer_create(world, object, 114, &mut light).code,
            0
        );
    }
    assert_eq!(
        disunity_light2d_color(world, light, 0.25, 0.5, 0.75, 1.).code,
        0
    );
    assert_ne!(
        disunity_light2d_color(world, light, 0., 0., f32::NAN, 1.).code,
        0
    );
    let mut red = 0.;
    unsafe {
        assert_eq!(disunity_light2d_read(world, light, 0, &mut red).code, 0);
    }
    assert_eq!(red.to_bits(), 0.25_f32.to_bits());
    let layers = [7, -2];
    let (mut written, mut output) = (0, [0; 2]);
    unsafe {
        assert_eq!(
            disunity_light2d_layers(world, light, layers.as_ptr(), 2).code,
            0
        );
        assert_ne!(
            disunity_light2d_layers_read(world, light, output.as_mut_ptr(), 1, &mut written).code,
            0
        );
        assert_eq!(
            disunity_light2d_layers_read(world, light, output.as_mut_ptr(), 2, &mut written).code,
            0
        );
    }
    assert_eq!(output, layers);
    assert_eq!(written, 2);
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn unity_camera_override_numbers_survive_the_abi() {
    let (world, object) = world_object();
    let mut camera = 0;
    unsafe {
        assert_eq!(
            disunity_camera_layer_create(world, object, 115, &mut camera).code,
            0
        );
    }
    for property in [8, 9] {
        for value in [0, 2] {
            assert_eq!(
                disunity_camera_data_write(world, camera, property, value).code,
                0
            );
            let rejected = disunity_camera_data_write(world, camera, property, 1);
            assert_eq!(
                rejected.detail,
                DisunityErrorReason::InvalidComponent as u64
            );
            let mut actual = 99;
            unsafe {
                assert_eq!(
                    disunity_camera_data_read(world, camera, property, &mut actual).code,
                    0
                );
            }
            assert_eq!(actual, value);
        }
    }
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
