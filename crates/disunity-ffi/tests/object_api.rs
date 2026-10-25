use worldrunner_disunity_ffi::{abi::*, object_api::*, result_code, with_native_world};

fn object() -> (u64, u64) {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
    }
    (world, object)
}

#[test]
fn bounded_utf8_metadata_updates_the_native_world_without_partial_output() {
    let (world, object) = object();
    let text = "actor λ".as_bytes();
    let mut output = [0xa5; 32];
    let mut written = 0;
    unsafe {
        assert_eq!(
            disunity_object_text_write(world, object, 1, text.as_ptr(), text.len() as u32).code,
            result_code::OK
        );
        assert_ne!(
            disunity_object_text_read(world, object, 1, output.as_mut_ptr(), 1, &mut written).code,
            result_code::OK
        );
        assert_eq!(output, [0xa5; 32]);
        assert_eq!(written as usize, text.len());
        assert_eq!(
            disunity_object_text_read(world, object, 1, output.as_mut_ptr(), 32, &mut written).code,
            result_code::OK
        );
        assert_eq!(&output[..written as usize], text);
        assert_eq!(
            disunity_object_text_write(world, object, 1, [0xff].as_ptr(), 1).code,
            result_code::INVALID_ARGUMENT
        );
    }
    with_native_world(world, |native| {
        assert_eq!(native.snapshot().objects[0].tag, "actor λ")
    })
    .unwrap();
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}

#[test]
fn invalid_transform_does_not_publish_partial_native_values() {
    let (world, object) = object();
    let mut value = DisunityTransform::default();
    unsafe {
        assert_eq!(
            disunity_transform_read(world, object, &mut value).code,
            result_code::OK
        );
        assert_eq!(value.sx, 1.0);
        value.sx = 3.0;
        value.qw = 0.0;
        assert_ne!(
            disunity_transform_write(world, object, &value).code,
            result_code::OK
        );
        assert_eq!(
            disunity_transform_read(world, object, &mut value).code,
            result_code::OK
        );
        assert_eq!(value.sx, 1.0);
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
        assert_ne!(
            disunity_transform_read(world, object, &mut value).code,
            result_code::OK
        );
    }
}
