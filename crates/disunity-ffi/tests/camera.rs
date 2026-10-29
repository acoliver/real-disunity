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
        ..DisunityCamera::default()
    }
}

fn write_matrix(world: u64, camera: u64, kind: u32, matrix: Option<[f32; 16]>) -> DisunityResult {
    let pointer = matrix.as_ref().map_or(std::ptr::null(), |m| m.as_ptr());
    unsafe { disunity_camera_matrix_write(world, camera, kind, pointer) }
}

fn read_matrix(world: u64, camera: u64, kind: u32) -> Option<[f32; 16]> {
    let (mut matrix, mut is_set) = ([0.; 16], 7);
    let result = unsafe {
        disunity_camera_matrix_read(world, camera, kind, matrix.as_mut_ptr(), &mut is_set)
    };
    assert_eq!(result.code, 0);
    assert!(is_set <= 1);
    (is_set == 1).then_some(matrix)
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

/// Settings the native camera refuses, each differing from [`pong`] in one way.
fn invalid_settings() -> [DisunityCamera; 10] {
    [
        DisunityCamera {
            orthographic: 2,
            ..pong()
        },
        DisunityCamera {
            field_of_view: 180.,
            ..pong()
        },
        DisunityCamera {
            viewport_width: -1.,
            ..pong()
        },
        DisunityCamera {
            allow_hdr: 2,
            ..pong()
        },
        DisunityCamera {
            camera_type: 0,
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
    ]
}

#[test]
fn unsupported_and_invalid_settings_are_typed_and_leave_the_camera_unchanged() {
    let (world, camera) = camera_world();
    assert_eq!(write(world, camera, pong()).code, 0);
    let rejected = invalid_settings();
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
                field_of_view: 180.,
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

#[test]
fn a_new_camera_reads_back_unitys_defaults() {
    let (world, camera) = camera_world();
    let value = read(world, camera);
    assert_eq!((value.orthographic, value.field_of_view), (1, 60.));
    assert_eq!(value.culling_mask, u32::MAX);
    let viewport = [
        value.viewport_x,
        value.viewport_y,
        value.viewport_width,
        value.viewport_height,
    ];
    assert_eq!(viewport, [0., 0., 1., 1.]);
    let flags = [
        value.allow_hdr,
        value.allow_msaa,
        value.allow_dynamic_resolution,
        value.use_occlusion_culling,
    ];
    assert_eq!(flags, [1, 1, 0, 1]);
    assert_eq!(
        (
            value.depth_texture_mode,
            value.target_display,
            value.target_texture
        ),
        (0, 0, 0)
    );
    assert_eq!(value.camera_type, 1);
    assert_eq!(read_matrix(world, camera, 0), None);
    assert_eq!(read_matrix(world, camera, 1), None);
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn every_added_setting_round_trips() {
    let (world, camera) = camera_world();
    let value = DisunityCamera {
        orthographic: 0,
        field_of_view: 42.5,
        target_texture: 0x1_0000_0002,
        culling_mask: 0b1010,
        viewport_x: 0.25,
        viewport_y: 0.5,
        viewport_width: 0.5,
        viewport_height: 0.125,
        depth_texture_mode: 3,
        allow_hdr: 1,
        allow_msaa: 0,
        allow_dynamic_resolution: 1,
        use_occlusion_culling: 0,
        target_display: 3,
        camera_type: 5,
        ..pong()
    };
    assert_eq!(write(world, camera, value).code, 0);
    let stored = read(world, camera);
    assert_eq!(stored.orthographic, 0);
    assert_eq!(
        stored.orthographic_size, 8.,
        "the orthographic size survives a perspective lens"
    );
    assert_eq!(stored.field_of_view, 42.5);
    assert_eq!(stored.target_texture, 0x1_0000_0002);
    assert_eq!(stored.culling_mask, 0b1010);
    assert_eq!(
        [
            stored.viewport_x,
            stored.viewport_y,
            stored.viewport_width,
            stored.viewport_height
        ],
        [0.25, 0.5, 0.5, 0.125]
    );
    assert_eq!(stored.depth_texture_mode, 3);
    assert_eq!(
        [
            stored.allow_hdr,
            stored.allow_msaa,
            stored.allow_dynamic_resolution,
            stored.use_occlusion_culling
        ],
        [1, 0, 1, 0]
    );
    assert_eq!((stored.target_display, stored.camera_type), (3, 5));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn custom_matrices_are_stored_cleared_and_kept_by_record_writes() {
    let (world, camera) = camera_world();
    let projection: [f32; 16] = std::array::from_fn(|i| i as f32 + 0.5);
    let view: [f32; 16] = std::array::from_fn(|i| -(i as f32));
    assert_eq!(write_matrix(world, camera, 0, Some(projection)).code, 0);
    assert_eq!(write_matrix(world, camera, 1, Some(view)).code, 0);
    assert_eq!(write(world, camera, pong()).code, 0);
    assert_eq!(read_matrix(world, camera, 0), Some(projection));
    assert_eq!(read_matrix(world, camera, 1), Some(view));
    assert_eq!(write_matrix(world, camera, 0, None).code, 0);
    assert_eq!(read_matrix(world, camera, 0), None);
    assert_eq!(read_matrix(world, camera, 1), Some(view));
    let mut bad = projection;
    bad[3] = f32::NAN;
    assert_ne!(write_matrix(world, camera, 1, Some(bad)).code, 0);
    assert_eq!(
        read_matrix(world, camera, 1),
        Some(view),
        "a rejected matrix changes nothing"
    );
    assert_ne!(
        write_matrix(world, camera, 2, None).code,
        0,
        "unknown matrix kind"
    );
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}

#[test]
fn a_perspective_camera_is_accepted_and_the_frame_names_what_it_cannot_show() {
    let (world, camera) = camera_world();
    let perspective = DisunityCamera {
        orthographic: 0,
        ..pong()
    };
    assert_eq!(write(world, camera, perspective).code, 0);
    let rejected = [
        perspective,
        DisunityCamera {
            viewport_width: 0.5,
            ..pong()
        },
        DisunityCamera {
            target_texture: 9,
            ..pong()
        },
        DisunityCamera {
            target_display: 1,
            ..pong()
        },
    ];
    for value in rejected {
        assert_eq!(write(world, camera, value).code, 0, "{value:?}");
        let error = clear_color(world).unwrap_err();
        assert_eq!(error.detail, DisunityErrorReason::InvalidComponent as u64);
    }
    assert_eq!(write(world, camera, pong()).code, 0);
    assert_eq!(write_matrix(world, camera, 0, Some([1.; 16])).code, 0);
    assert!(clear_color(world).is_err(), "a custom projection matrix");
    assert_eq!(write_matrix(world, camera, 0, None).code, 0);
    assert_eq!(write_matrix(world, camera, 1, Some([1.; 16])).code, 0);
    assert!(clear_color(world).is_err(), "a custom view matrix");
    assert_eq!(write_matrix(world, camera, 1, None).code, 0);
    assert_eq!(clear_color(world), Ok([0., 0., 0., 0.]));
    unsafe { assert_eq!(disunity_world_destroy(world).code, 0) };
}
