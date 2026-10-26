use worldrunner_disunity_ffi::{
    abi::{disunity_world_create, disunity_world_destroy},
    input_axes::{
        DisunityInputAxis, disunity_input_axis_id, disunity_input_axis_read,
        disunity_input_axis_register,
    },
};

fn config() -> DisunityInputAxis {
    DisunityInputAxis {
        negative: 1,
        positive: 2,
        alt_negative: 65536,
        alt_positive: 65536,
        gravity: 3.0,
        sensitivity: 3.0,
        dead: 0.001,
        flags: 1,
    }
}
fn with_world(test: impl FnOnce(u64)) {
    let mut world = 0;
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(
            disunity_input_axis_register(world, 7, b"axis".as_ptr(), 4, &config()).code,
            0
        );
    }
    test(world);
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
#[test]
fn reads_registered_id_and_initial_value() {
    with_world(|world| unsafe {
        let mut id = 999;
        let mut value = 123.0;
        assert_eq!(
            disunity_input_axis_id(world, b"axis".as_ptr(), 4, &mut id).code,
            0
        );
        assert_eq!(id, 7);
        assert_eq!(
            disunity_input_axis_read(world, 0, b"axis".as_ptr(), 4, 1, &mut value).code,
            0
        );
        assert_eq!(value, 0.0);
    });
}
#[test]
fn invalid_config_and_key_preserve_registration() {
    with_world(|world| unsafe {
        let mut id = 0;
        let mut bad = config();
        bad.gravity = f32::NAN;
        assert_ne!(
            disunity_input_axis_register(world, 7, b"bad".as_ptr(), 3, &bad).code,
            0
        );
        assert_eq!(
            disunity_input_axis_id(world, b"axis".as_ptr(), 4, &mut id).code,
            0
        );
        bad = config();
        bad.positive = 65537;
        assert_ne!(
            disunity_input_axis_register(world, 7, b"axis".as_ptr(), 4, &bad).code,
            0
        );
        assert_eq!(
            disunity_input_axis_id(world, b"axis".as_ptr(), 4, &mut id).code,
            0
        );
    });
}
#[test]
fn invalid_foreign_names_ids_and_phases_are_errors() {
    with_world(|world| unsafe {
        let mut id = 0;
        let mut value = 0.0;
        assert_ne!(
            disunity_input_axis_register(world, 65536, b"axis".as_ptr(), 4, &config()).code,
            0
        );
        assert_ne!(
            disunity_input_axis_register(world, 8, [255].as_ptr(), 1, &config()).code,
            0
        );
        assert_ne!(
            disunity_input_axis_register(world, 8, std::ptr::null(), 1, &config()).code,
            0
        );
        assert_ne!(
            disunity_input_axis_read(world, 2, b"axis".as_ptr(), 4, 0, &mut value).code,
            0
        );
        assert_ne!(
            disunity_input_axis_id(world, b"missing".as_ptr(), 7, &mut id).code,
            0
        );
    });
}
