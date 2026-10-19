//! Behavioural claims about the single Z-up to Y-up basis change.

use super::{CameraPose, SceneView, ViewError, Viewport};
use glam::Vec3;
use worldrunner_render_plan::SceneCamera;

/// The tolerance for comparing single-precision matrix products.
const EPSILON: f32 = 1e-4;

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 90_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn viewport() -> Viewport {
    Viewport {
        width: 1_920,
        height: 1_080,
    }
}

/// Builds a view from an eye standing south of the origin at head height,
/// looking north along `+Y` and level with the horizon.
fn level_view() -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        camera(),
        viewport(),
    )
    .expect("a level northward pose determines a view")
}

fn assert_close(actual: Vec3, expected: Vec3, claim: &str) {
    assert!(
        (actual - expected).length() < EPSILON,
        "{claim}: expected {expected:?} but found {actual:?}"
    );
}

#[test]
fn the_world_up_axis_maps_to_view_up() {
    // The whole purpose of the basis change: authored +Z must leave the view
    // matrix pointing along view-space +Y. If this drifts, every later stage
    // compensates for it in its own way.
    let view = level_view();
    let up = view.view_matrix().transform_vector3(Vec3::Z);
    assert_close(up, Vec3::Y, "authored up becomes view up");
}

#[test]
fn the_view_direction_maps_to_view_forward() {
    // A right-handed view space looks down its own -Z.
    let view = level_view();
    let forward = view.view_matrix().transform_vector3(Vec3::Y);
    assert_close(forward, -Vec3::Z, "the look direction becomes view forward");
}

#[test]
fn the_basis_change_preserves_handedness() {
    // A basis change performed as an axis swap rather than a rotation flips
    // handedness, which silently converts back-face culling into front-face
    // culling and mirrors every tangent-space normal. Determinant +1 is the
    // property that distinguishes the two, and no amount of visual inspection
    // of a symmetric test scene would reveal the difference.
    let determinant = level_view().view_matrix().determinant();
    assert!(
        (determinant - 1.0).abs() < EPSILON,
        "the view basis must be a rotation, but its determinant is {determinant}"
    );
}

#[test]
fn the_eye_position_maps_to_the_view_origin() {
    let view = level_view();
    let origin = view
        .view_matrix()
        .transform_point3(Vec3::new(0.0, -10.0, 1.7));
    assert_close(origin, Vec3::ZERO, "the eye sits at the view origin");
}

#[test]
fn scene_millimeters_are_reported_as_metres() {
    // Millimetres would exhaust f32 precision a few kilometres from the
    // origin; the conversion to metres is what keeps distant geometry stable.
    assert_close(
        level_view().eye_meters(),
        Vec3::new(0.0, -10.0, 1.7),
        "the eye is reported in metres",
    );
}

#[test]
fn the_eye_is_reported_in_scene_space_not_view_space() {
    // Shading happens against geometry that is still Z-up, so the eye must be
    // reported in that same space. Returning a view-space eye would be
    // indistinguishable at the origin and wrong everywhere else.
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [3_000, -10_000, 1_700],
            target_millimeters: [3_000, 0, 1_700],
        },
        camera(),
        viewport(),
    )
    .expect("an offset pose determines a view");
    assert_close(
        view.eye_meters(),
        Vec3::new(3.0, -10.0, 1.7),
        "the eye keeps its authored scene coordinates",
    );
}

#[test]
fn a_point_on_the_view_axis_lands_at_the_clip_centre() {
    let view = level_view();
    let clip = view
        .view_projection_matrix()
        .project_point3(Vec3::new(0.0, 0.0, 1.7));
    assert!(
        clip.x.abs() < EPSILON && clip.y.abs() < EPSILON,
        "a point straight ahead must project to the centre, but found {clip:?}"
    );
}

#[test]
fn depth_spans_the_zero_to_one_range_wgpu_uses() {
    // A projection built for OpenGL's -1..=1 depth range still centres
    // correctly and still renders; it simply loses half the depth buffer. The
    // near-plane depth is what distinguishes the two conventions.
    let view = level_view();
    let near = view
        .view_projection_matrix()
        .project_point3(Vec3::new(0.0, -9.9, 1.7));
    assert!(
        near.z >= 0.0 && near.z < 0.01,
        "the near plane must map to depth zero, but found {}",
        near.z
    );
}

#[test]
fn something_above_the_camera_appears_above_the_centre() {
    // Guards the sign of the up axis end to end: an inverted basis change
    // still passes an axis-alignment check while rendering the world upside
    // down.
    let view = level_view();
    let clip = view
        .view_projection_matrix()
        .project_point3(Vec3::new(0.0, 0.0, 3.7));
    assert!(
        clip.y > 0.0,
        "a point above the eye must project above centre, but found {clip:?}"
    );
}

#[test]
fn something_east_of_the_camera_appears_right_of_the_centre() {
    // The companion sign check. Together with the up test this pins the
    // orientation completely, so a mirrored image cannot pass.
    let view = level_view();
    let clip = view
        .view_projection_matrix()
        .project_point3(Vec3::new(2.0, 0.0, 1.7));
    assert!(
        clip.x > 0.0,
        "a point east of a northward eye must project right of centre, but found {clip:?}"
    );
}

#[test]
fn a_coincident_eye_and_target_are_rejected() {
    let error = SceneView::new(
        CameraPose {
            eye_millimeters: [1, 2, 3],
            target_millimeters: [1, 2, 3],
        },
        camera(),
        viewport(),
    )
    .expect_err("a zero-length view direction cannot describe a view");
    assert_eq!(error, ViewError::DegenerateDirection);
}

#[test]
fn looking_straight_down_is_rejected_rather_than_given_an_invented_roll() {
    // A top-down view is a perfectly reasonable thing to want, but the roll
    // about the view axis is genuinely undetermined by the pose. Choosing one
    // silently would make the captured image depend on an unstated convention,
    // so the caller is asked to state it instead.
    let error = SceneView::new(
        CameraPose {
            eye_millimeters: [0, 0, 10_000],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        viewport(),
    )
    .expect_err("a vertical view direction leaves roll unconstrained");
    assert_eq!(error, ViewError::DegenerateRoll);
}

#[test]
fn a_viewport_without_area_is_rejected() {
    let error = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        Viewport {
            width: 1_920,
            height: 0,
        },
    )
    .expect_err("a zero-height viewport has no aspect ratio");
    assert_eq!(
        error,
        ViewError::DegenerateViewport {
            width: 1_920,
            height: 0,
        }
    );
}

#[test]
fn the_aspect_ratio_widens_the_horizontal_field_of_view() {
    // The vertical field of view is the authored one, so a wider viewport must
    // show more sideways, not squash what is already there. Transposing the
    // aspect ratio passes every centred test and distorts every real capture.
    let wide = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        Viewport {
            width: 1_920,
            height: 1_080,
        },
    )
    .expect("a wide viewport determines a view");
    let square = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        Viewport {
            width: 1_080,
            height: 1_080,
        },
    )
    .expect("a square viewport determines a view");

    let sample = Vec3::new(2.0, 0.0, 0.0);
    let wide_x = wide.view_projection_matrix().project_point3(sample).x;
    let square_x = square.view_projection_matrix().project_point3(sample).x;
    assert!(
        wide_x < square_x,
        "a wider viewport must push a fixed point toward the centre, but {wide_x} >= {square_x}"
    );

    let overhead = Vec3::new(0.0, 0.0, 2.0);
    let wide_y = wide.view_projection_matrix().project_point3(overhead).y;
    let square_y = square.view_projection_matrix().project_point3(overhead).y;
    assert!(
        (wide_y - square_y).abs() < EPSILON,
        "the authored vertical field of view must not depend on viewport width, \
         but {wide_y} differs from {square_y}"
    );
}
