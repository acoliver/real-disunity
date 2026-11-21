//! Which renderers cast a light's shadows, and the view, projection, sphere and planes a shadow
//! map of each kind of light is drawn with. The camera and lights follow the engine's
//! conventions: a left-handed world, a camera looking down negative z in its view matrix, and a
//! light shining along its own z axis.

use glam::{Mat4, Quat, Vec3, Vec4};
use worldrunner_world_simulation::shadows::{
    Casters, DirectionalSplit, ShadowCamera, ShadowCaster, ShadowError, ShadowLight,
    ShadowLightKind, ShadowSplit, caster_bounds, directional_split, point_split, spot_split,
};

const ON: u32 = 1;
const OFF: u32 = 0;
const EPSILON: f32 = 1e-3;

fn cube(centre: Vec3, half: f32, mode: u32) -> ShadowCaster {
    ShadowCaster {
        min: centre - Vec3::splat(half),
        max: centre + Vec3::splat(half),
        casting_mode: mode,
        layer: 0,
    }
}

fn casters(list: &[ShadowCaster]) -> Casters<'_> {
    Casters {
        list,
        culling_mask: u32::MAX,
    }
}

/// A camera at `position` looking down +z with a 60 degree vertical view of a square image, as the
/// engine builds its matrices: the view flips z and the projection puts depth at -1 to 1.
fn camera_at(position: Vec3, shadow_distance: f32) -> ShadowCamera {
    let view = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)) * Mat4::from_translation(-position);
    let (near, far) = (0.3_f32, 100.0_f32);
    let scale = 1.0 / (30.0_f32.to_radians()).tan();
    let projection = Mat4::from_cols(
        Vec4::new(scale, 0.0, 0.0, 0.0),
        Vec4::new(0.0, scale, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -(far + near) / (far - near), -1.0),
        Vec4::new(0.0, 0.0, -2.0 * far * near / (far - near), 0.0),
    );
    ShadowCamera {
        culling_matrix: projection * view,
        origin: position,
        shadow_distance,
    }
}

fn sun() -> ShadowLight {
    // Euler(50, -30, 0) in the engine: yaw -30 degrees after pitching 50 degrees down.
    let rotation = Quat::from_rotation_y(-30.0_f32.to_radians())
        * Quat::from_rotation_x(50.0_f32.to_radians());
    ShadowLight {
        kind: ShadowLightKind::Directional,
        local_to_world: Mat4::from_rotation_translation(rotation, Vec3::new(0.0, 50.0, 0.0)),
        range: 0.0,
        spot_angle: 0.0,
    }
}

fn split(index: u32, count: u32) -> DirectionalSplit {
    DirectionalSplit {
        index,
        count,
        ratio: Vec3::new(0.5, 0.0, 0.0),
        resolution: 1024,
        near_plane_offset: 3.0,
    }
}

fn corners(centre: Vec3, half: f32) -> Vec<Vec3> {
    (0..8)
        .map(|bit| {
            let sign = |axis: u32| if (bit >> axis) & 1 == 1 { half } else { -half };
            centre + Vec3::new(sign(0), sign(1), sign(2))
        })
        .collect()
}

fn clip(split: &ShadowSplit, point: Vec3) -> Vec3 {
    (split.projection * split.view).project_point3(point)
}

fn inside_clip_box(split: &ShadowSplit, point: Vec3) -> bool {
    let clip = clip(split, point);
    clip.x.abs() <= 1.0 + EPSILON && clip.y.abs() <= 1.0 + EPSILON && clip.z.abs() <= 1.0 + EPSILON
}

fn inside_sphere(split: &ShadowSplit, point: Vec3) -> bool {
    let sphere = split.culling_sphere;
    point.distance(sphere.truncate()) <= sphere.w + EPSILON
}

fn inside_planes(split: &ShadowSplit, point: Vec3) -> bool {
    split
        .planes
        .iter()
        .all(|plane| plane.truncate().dot(point) + plane.w >= -EPSILON)
}

#[test]
fn directional_caster_bounds_hold_every_caster_that_casts() {
    let list = [
        cube(Vec3::new(0.0, 0.0, 10.0), 1.0, ON),
        cube(Vec3::new(40.0, 0.0, 0.0), 2.0, ON),
        cube(Vec3::new(-300.0, 0.0, 0.0), 1.0, OFF),
    ];
    let (min, max) = caster_bounds(&sun(), &casters(&list)).unwrap();
    assert_eq!(min, Vec3::new(-1.0, -2.0, -2.0));
    assert_eq!(max, Vec3::new(42.0, 2.0, 11.0));
}

#[test]
fn no_caster_that_casts_gives_no_bounds() {
    assert_eq!(caster_bounds(&sun(), &casters(&[])), None);
    let list = [cube(Vec3::ZERO, 1.0, OFF)];
    assert_eq!(caster_bounds(&sun(), &casters(&list)), None);
}

#[test]
fn a_caster_on_a_layer_the_culling_left_out_does_not_count() {
    let mut caster = cube(Vec3::ZERO, 1.0, ON);
    caster.layer = 5;
    let drawn = Casters {
        list: &[caster],
        culling_mask: 1,
    };
    assert_eq!(caster_bounds(&sun(), &drawn), None);
    let drawn = Casters {
        list: &[caster],
        culling_mask: 1 << 5,
    };
    assert!(caster_bounds(&sun(), &drawn).is_some());
}

#[test]
fn a_point_light_counts_only_the_casters_within_its_range() {
    let light = ShadowLight {
        kind: ShadowLightKind::Point,
        local_to_world: Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0)),
        range: 10.0,
        spot_angle: 0.0,
    };
    let list = [
        cube(Vec3::new(0.0, 5.0, 8.0), 1.0, ON),
        cube(Vec3::new(0.0, 5.0, 30.0), 1.0, ON),
    ];
    let (min, max) = caster_bounds(&light, &casters(&list)).unwrap();
    assert_eq!((min.z, max.z), (7.0, 9.0));
    let far = [cube(Vec3::new(0.0, 5.0, 30.0), 1.0, ON)];
    assert_eq!(caster_bounds(&light, &casters(&far)), None);
}

#[test]
fn the_first_of_two_splits_holds_its_slice_and_the_casters_in_it() {
    let camera = camera_at(Vec3::ZERO, 80.0);
    let list = [cube(Vec3::new(0.0, 0.0, 10.0), 0.5, ON)];
    let first = directional_split(&sun(), &camera, &casters(&list), &split(0, 2))
        .unwrap()
        .unwrap();
    for point in corners(Vec3::new(0.0, 0.0, 10.0), 0.5) {
        assert!(
            inside_clip_box(&first, point),
            "{point:?} outside the clip box"
        );
        assert!(inside_sphere(&first, point), "{point:?} outside the sphere");
        assert!(inside_planes(&first, point), "{point:?} outside the planes");
    }
    // The slice runs from the near plane to half the shadow distance, 40 units.
    assert!(inside_sphere(&first, Vec3::new(0.0, 0.0, 40.0)));
    assert!(!inside_sphere(&first, Vec3::new(0.0, 0.0, 90.0)));
    assert!(first.planes.len() <= 10);
    assert_eq!(first.culling_matrix, first.projection * first.view);
}

#[test]
fn the_second_split_starts_where_the_first_ends() {
    let camera = camera_at(Vec3::ZERO, 80.0);
    let list = [cube(Vec3::new(0.0, 0.0, 60.0), 0.5, ON)];
    let second = directional_split(&sun(), &camera, &casters(&list), &split(1, 2))
        .unwrap()
        .unwrap();
    for point in corners(Vec3::new(0.0, 0.0, 60.0), 0.5) {
        assert!(inside_clip_box(&second, point), "{point:?}");
        assert!(inside_sphere(&second, point), "{point:?}");
    }
    assert!(inside_sphere(&second, Vec3::new(0.0, 0.0, 41.0)));
    assert!(inside_sphere(&second, Vec3::new(0.0, 0.0, 79.0)));
    assert!(!inside_sphere(&second, Vec3::new(0.0, 0.0, 5.0)));
}

#[test]
fn a_caster_between_the_light_and_the_slice_stays_inside_the_clip_depth() {
    let camera = camera_at(Vec3::ZERO, 80.0);
    // A caster 40 units up the light's beam from the slice shadows it without being in it.
    let toward_light = -sun().local_to_world.z_axis.truncate();
    let high = Vec3::new(0.0, 0.0, 10.0) + toward_light * 40.0;
    let list = [cube(high, 0.5, ON)];
    let first = directional_split(&sun(), &camera, &casters(&list), &split(0, 2))
        .unwrap()
        .unwrap();
    assert!(inside_clip_box(&first, high), "{:?}", clip(&first, high));
    assert!(inside_planes(&first, high));
}

#[test]
fn the_shadow_map_moves_by_whole_texels_when_the_camera_moves() {
    let list = [cube(Vec3::new(0.0, 0.0, 10.0), 0.5, ON)];
    let at = |x: f32| {
        directional_split(
            &sun(),
            &camera_at(Vec3::new(x, 0.0, 0.0), 80.0),
            &casters(&list),
            &split(0, 2),
        )
        .unwrap()
        .unwrap()
    };
    let base = at(0.0);
    let texel_x = 2.0 / base.projection.x_axis.x / 1024.0;
    for step in 1..20_u16 {
        let moved = at(f32::from(step) * 0.003);
        // The same orientation and scale, and a world point lands a whole number of texels away.
        assert_eq!(base.projection, moved.projection);
        let point = Vec3::new(1.0, 2.0, 3.0);
        let shift = (clip(&moved, point) - clip(&base, point)) * 512.0;
        let whole =
            (shift.x - shift.x.round()).abs() < 0.02 && (shift.y - shift.y.round()).abs() < 0.02;
        assert!(whole, "step {step}: shift {shift:?} texel {texel_x}");
    }
}

#[test]
fn a_directional_light_without_casters_has_no_split() {
    let camera = camera_at(Vec3::ZERO, 80.0);
    let list = [cube(Vec3::new(0.0, 0.0, 10.0), 0.5, OFF)];
    assert_eq!(
        directional_split(&sun(), &camera, &casters(&list), &split(0, 2)),
        Ok(None)
    );
}

#[test]
fn a_split_outside_the_count_the_ratios_or_the_resolution_is_refused() {
    let camera = camera_at(Vec3::ZERO, 80.0);
    let list = [cube(Vec3::new(0.0, 0.0, 10.0), 0.5, ON)];
    let run = |split: DirectionalSplit| directional_split(&sun(), &camera, &casters(&list), &split);
    assert_eq!(run(split(2, 2)), Err(ShadowError::InvalidSplit));
    assert_eq!(run(split(0, 0)), Err(ShadowError::InvalidSplit));
    assert_eq!(run(split(0, 5)), Err(ShadowError::InvalidSplit));
    let descending = DirectionalSplit {
        ratio: Vec3::new(0.6, 0.3, 0.0),
        ..split(0, 3)
    };
    assert_eq!(run(descending), Err(ShadowError::InvalidRatio));
    let tiny = DirectionalSplit {
        resolution: 2,
        ..split(0, 2)
    };
    assert_eq!(run(tiny), Err(ShadowError::InvalidResolution));
    let huge = DirectionalSplit {
        resolution: 70_000,
        ..split(0, 2)
    };
    assert_eq!(run(huge), Err(ShadowError::InvalidResolution));
}

fn spot() -> ShadowLight {
    ShadowLight {
        kind: ShadowLightKind::Spot,
        local_to_world: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0)),
        range: 20.0,
        spot_angle: 60.0,
    }
}

#[test]
fn a_spot_light_shadow_map_holds_the_casters_in_its_cone() {
    let list = [cube(Vec3::new(0.0, 0.0, 8.0), 0.5, ON)];
    let map = spot_split(&spot(), &casters(&list)).unwrap().unwrap();
    for point in corners(Vec3::new(0.0, 0.0, 8.0), 0.5) {
        assert!(inside_clip_box(&map, point), "{point:?}");
        assert!(inside_planes(&map, point), "{point:?}");
    }
    // A point off to the side, beyond the 30 degree half angle, is outside the planes.
    assert!(!inside_planes(&map, Vec3::new(30.0, 0.0, 8.0)));
    assert_eq!(map.culling_sphere, Vec4::ZERO);
}

#[test]
fn a_spot_light_with_no_caster_in_range_has_no_shadow_map() {
    let list = [cube(Vec3::new(0.0, 0.0, 80.0), 0.5, ON)];
    assert_eq!(spot_split(&spot(), &casters(&list)), Ok(None));
    let bad = ShadowLight {
        spot_angle: 0.0,
        ..spot()
    };
    let near = [cube(Vec3::new(0.0, 0.0, 8.0), 0.5, ON)];
    assert_eq!(
        spot_split(&bad, &casters(&near)),
        Err(ShadowError::InvalidLight)
    );
}

fn point() -> ShadowLight {
    ShadowLight {
        kind: ShadowLightKind::Point,
        local_to_world: Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)),
        range: 15.0,
        spot_angle: 0.0,
    }
}

#[test]
fn each_face_of_a_point_light_looks_along_its_axis() {
    let centre = Vec3::new(1.0, 2.0, 3.0);
    let directions = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
    let list: Vec<ShadowCaster> = directions
        .iter()
        .map(|direction| cube(centre + *direction * 6.0, 0.5, ON))
        .collect();
    for face in 0..6_u32 {
        let looking = usize::try_from(face).unwrap();
        let map = point_split(&point(), face, 0.0, &casters(&list))
            .unwrap()
            .unwrap();
        for (other, towards) in directions.iter().enumerate() {
            let inside = inside_clip_box(&map, centre + *towards * 6.0);
            assert_eq!(inside, other == looking, "face {face} looking at {other}");
        }
    }
    assert_eq!(
        point_split(&point(), 6, 0.0, &casters(&list)),
        Err(ShadowError::InvalidFace)
    );
}

#[test]
fn a_field_of_view_bias_widens_a_point_light_face() {
    let centre = Vec3::new(1.0, 2.0, 3.0);
    let list = [cube(centre + Vec3::new(0.0, 0.0, 6.0), 0.5, ON)];
    let plain = point_split(&point(), 4, 0.0, &casters(&list))
        .unwrap()
        .unwrap();
    let wide = point_split(&point(), 4, 10.0, &casters(&list))
        .unwrap()
        .unwrap();
    assert!(wide.projection.x_axis.x < plain.projection.x_axis.x);
}
