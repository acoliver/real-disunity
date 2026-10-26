use worldrunner_disunity_ffi::{abi::*, camera::*, presentation::*, sprites::*};

fn camera_world() -> (u64, u64) {
    let (mut world, mut object, mut camera) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
    }
    (world, camera)
}

fn read(world: u64, camera: u64) -> DisunityCamera {
    let mut value = DisunityCamera::default();
    assert_eq!(
        unsafe { disunity_camera_read(world, camera, &mut value) }.code,
        0
    );
    value
}

fn write(world: u64, camera: u64, value: DisunityCamera) -> DisunityResult {
    unsafe { disunity_camera_write(world, camera, &value) }
}

/// The settings Pong's camera serializes: Skybox with no skybox material, ortho size 8.
fn pong() -> DisunityCamera {
    DisunityCamera {
        enabled: 1,
        clear_flags: 1,
        orthographic: 1,
        depth: -1,
        orthographic_size: 8.,
        near: 0.3,
        far: 1000.,
        r: 0.,
        g: 0.,
        b: 0.,
        a: 0.,
    }
}

fn clear_color(world: u64) -> Result<[f64; 4], DisunityResult> {
    let mut header = DisunityPresentation::default();
    let result = unsafe { disunity_presentation_prepare(world, 64, 64, 1, &mut header) };
    if result.code != 0 {
        return Err(result);
    }
    Ok([header.r, header.g, header.b, header.a])
}

#[test]
fn serialized_camera_settings_round_trip_and_reach_the_frame() {
    let (world, camera) = camera_world();
    let defaults = read(world, camera);
    assert_eq!((defaults.clear_flags, defaults.orthographic), (2, 1));
    assert_eq!(write(world, camera, pong()).code, 0);
    let stored = read(world, camera);
    assert_eq!(
        (stored.clear_flags, stored.depth, stored.orthographic_size),
        (1, -1, 8.)
    );
    assert_eq!((stored.near, stored.far), (0.3, 1000.));
    assert_eq!(clear_color(world), Ok([0., 0., 0., 0.]));
    let breakout = DisunityCamera {
        clear_flags: 2,
        r: 0.25,
        a: 1.,
        ..pong()
    };
    assert_eq!(write(world, camera, breakout).code, 0);
    assert_eq!(clear_color(world), Ok([0.25, 0., 0., 1.]));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn unsupported_and_invalid_settings_are_typed_and_leave_the_camera_unchanged() {
    let (world, camera) = camera_world();
    assert_eq!(write(world, camera, pong()).code, 0);
    let rejected = [
        DisunityCamera {
            orthographic: 0,
            ..pong()
        },
        DisunityCamera {
            clear_flags: 5,
            ..pong()
        },
        DisunityCamera {
            orthographic_size: 0.,
            ..pong()
        },
        DisunityCamera { far: 0.2, ..pong() },
        DisunityCamera { r: 1.5, ..pong() },
        DisunityCamera {
            enabled: 2,
            ..pong()
        },
    ];
    for value in rejected {
        let result = write(world, camera, value);
        assert_ne!(result.code, 0, "{value:?}");
        assert_eq!(read(world, camera).orthographic_size, 8.);
        assert_eq!(read(world, camera).clear_flags, 1);
    }
    assert_eq!(
        write(
            world,
            camera,
            DisunityCamera {
                orthographic: 0,
                ..pong()
            }
        )
        .detail,
        DisunityErrorReason::InvalidComponent as u64
    );
    assert_eq!(
        unsafe { disunity_camera_write(world, camera, std::ptr::null()) }.code,
        result_code::INVALID_ARGUMENT
    );
    let mut sprite = 0;
    let mut object = 0;
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_sprite_create(world, object, &mut sprite).code, 0);
    }
    assert_ne!(write(world, sprite, pong()).code, 0);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn clear_modes_that_need_a_lower_camera_are_rejected_at_presentation() {
    for flags in [3, 4] {
        let (world, camera) = camera_world();
        let value = DisunityCamera {
            clear_flags: flags,
            ..pong()
        };
        assert_eq!(write(world, camera, value).code, 0);
        let error = clear_color(world).unwrap_err();
        assert_eq!(error.detail, DisunityErrorReason::InvalidComponent as u64);
        unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
    }
}
