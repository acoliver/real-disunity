//! The lens and screen camera a prepared frame reports in its header.
use worldrunner_disunity_ffi::{abi::*, camera::*, presentation::*, sprites::*};

fn world_with(cameras: &[DisunityCamera]) -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    for value in cameras {
        let (mut object, mut camera) = (0, 0);
        unsafe {
            assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
            assert_eq!(disunity_camera_create(world, object, &mut camera).code, 0);
            assert_eq!(disunity_camera_write(world, camera, value).code, 0);
        }
    }
    world
}

fn prepare(world: u64) -> Result<DisunityPresentation, DisunityResult> {
    let mut header = DisunityPresentation::default();
    let result = unsafe { disunity_presentation_prepare(world, 64, 32, 1, &mut header) };
    if result.code == 0 {
        Ok(header)
    } else {
        Err(result)
    }
}

fn orthographic(depth: i32) -> DisunityCamera {
    DisunityCamera {
        enabled: 1,
        clear_flags: 2,
        orthographic: 1,
        depth,
        orthographic_size: 8.,
        near: 0.3,
        far: 1000.,
        a: 1.,
        ..DisunityCamera::default()
    }
}

fn perspective(depth: i32) -> DisunityCamera {
    DisunityCamera {
        orthographic: 0,
        field_of_view: 90.,
        near: 0.5,
        far: 200.,
        ..orthographic(depth)
    }
}

#[test]
fn an_orthographic_camera_reports_lens_kind_zero_and_its_size() {
    let header = prepare(world_with(&[orthographic(0)])).unwrap();
    assert_eq!((header.present, header.lens), (1, 0));
    assert_eq!(header.half_height, 8.);
    assert_eq!((header.near, header.far), (0.3f32 as f64, 1000.));
    assert_eq!(header.vertical_fov, 0.);
}

#[test]
fn a_perspective_camera_reports_lens_kind_one_and_its_field_of_view_in_radians() {
    let header = prepare(world_with(&[perspective(0)])).unwrap();
    assert_eq!((header.present, header.lens), (1, 1));
    assert!((header.vertical_fov - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
    assert_eq!((header.near, header.far), (0.5, 200.));
    assert_eq!(header.half_height, 0.);
}

#[test]
fn the_higher_depth_screen_camera_sets_the_lens() {
    let header = prepare(world_with(&[orthographic(0), perspective(3)])).unwrap();
    assert_eq!(header.lens, 1);
    let header = prepare(world_with(&[perspective(-3), orthographic(0)])).unwrap();
    assert_eq!(header.lens, 0);
}

#[test]
fn cameras_drawing_elsewhere_do_not_make_a_frame_camera() {
    let into_texture = DisunityCamera {
        target_texture: 9,
        ..perspective(5)
    };
    let second_display = DisunityCamera {
        target_display: 1,
        ..perspective(6)
    };
    let header = prepare(world_with(&[into_texture, second_display])).unwrap();
    assert_eq!(header.present, 0);
    let header = prepare(world_with(&[into_texture, orthographic(-1)])).unwrap();
    assert_eq!((header.present, header.lens), (1, 0));
}

#[test]
fn a_perspective_camera_without_a_near_plane_names_why_the_frame_fails() {
    let flat = DisunityCamera {
        near: 0.,
        ..perspective(0)
    };
    let error = prepare(world_with(&[flat])).unwrap_err();
    assert_eq!(error.detail, DisunityErrorReason::InvalidComponent as u64);
}
