//! Converts the object matrices and the camera of a presented frame into scene space.
//!
//! The presentation already puts both in the frame's view basis (Z up, left handed), the basis
//! the player's mesh vertices are drawn in once their object matrix is applied, so the matrix
//! decomposes into the scene's own placement without a further basis change.

use worldrunner_camera::{CameraLens, CameraState};
use worldrunner_render_plan::{SceneCamera, SceneRotation, SceneTransform};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::OrbitLimits;

use crate::player_meshes::round_to_i64;

const MICRO: f64 = 1_000_000.0;
const MILLIMETERS_PER_METER: f64 = 1_000.0;
/// Columns shorter than this have no direction to turn.
const MIN_SCALE: f64 = 1.0e-9;
/// How far two axes may stray from perpendicular, and two scales from equal, before the matrix
/// counts as sheared or non-uniformly scaled. A matrix built from `f32` parts is exact to a few
/// parts in ten million.
const TOLERANCE: f64 = 1.0e-3;
/// The distance ahead of the eye that the orbit centres on when the view meets no ground.
const LEVEL_TARGET_METERS: f64 = 10.0;
const MIN_TARGET_METERS: f64 = 1.0;
const MAX_TARGET_METERS: f64 = 100_000.0;
/// The steepest angle the orbit target may sit above or below the eye. The orbit refuses poses
/// past its elevation limit, so a steeper view looks from further out instead.
const MAX_VIEW_ELEVATION: f64 = 1.4;

/// Why a matrix has no scene placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransformError {
    #[error("the matrix has a zero scale")]
    ZeroScale,
    #[error("the matrix shears the mesh")]
    Shear,
    #[error("the matrix scales the axes differently, and the scene scales uniformly")]
    NonUniformScale,
    #[error("the matrix mirrors the mesh")]
    Mirrored,
    #[error("the matrix holds a value beyond the scene's range")]
    OutOfRange,
}

type Vec3 = [f64; 3];

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

fn column(matrix: &[f32; 16], index: usize) -> Vec3 {
    let start = index * 4;
    [
        f64::from(matrix[start]),
        f64::from(matrix[start + 1]),
        f64::from(matrix[start + 2]),
    ]
}

/// Splits a column-major affine matrix into the scene's millimeter position, rotation and
/// uniform scale. A matrix with shear, a zero scale, different scales per axis or a mirror has
/// no such placement.
pub fn scene_transform(matrix: &[f32; 16]) -> Result<SceneTransform, TransformError> {
    if matrix.iter().any(|value| !value.is_finite()) {
        return Err(TransformError::OutOfRange);
    }
    let columns = [column(matrix, 0), column(matrix, 1), column(matrix, 2)];
    let scales = columns.map(length);
    if scales.iter().any(|scale| *scale < MIN_SCALE) {
        return Err(TransformError::ZeroScale);
    }
    let axes: [Vec3; 3] = std::array::from_fn(|i| columns[i].map(|part| part / scales[i]));
    if [(0, 1), (0, 2), (1, 2)]
        .iter()
        .any(|(a, b)| dot(axes[*a], axes[*b]).abs() > TOLERANCE)
    {
        return Err(TransformError::Shear);
    }
    let mean = scales.iter().sum::<f64>() / 3.0;
    if scales
        .iter()
        .any(|scale| (scale - mean).abs() > TOLERANCE * mean)
    {
        return Err(TransformError::NonUniformScale);
    }
    if dot(cross(axes[0], axes[1]), axes[2]) < 0.0 {
        return Err(TransformError::Mirrored);
    }
    let scale = round_to_i64(mean * MICRO).and_then(|micro| u64::try_from(micro).ok());
    let position = [12, 13, 14].map(|at| {
        round_to_i64(f64::from(matrix[at]) * MILLIMETERS_PER_METER)
            .ok_or(TransformError::OutOfRange)
    });
    Ok(SceneTransform {
        position_millimeters: [position[0]?, position[1]?, position[2]?],
        rotation: rotation(axes)?,
        scale_millionths: scale
            .filter(|scale| *scale > 0)
            .ok_or(TransformError::ZeroScale)?,
    })
}

/// The unit quaternion, with `w` not negative, of the rotation whose columns are `axes`.
fn rotation(axes: [Vec3; 3]) -> Result<SceneRotation, TransformError> {
    // `entry(row, column)` of the rotation matrix.
    let entry = |row: usize, col: usize| axes[col][row];
    let trace = entry(0, 0) + entry(1, 1) + entry(2, 2);
    let parts: [f64; 4] = if trace > 0.0 {
        let norm = (trace + 1.0).sqrt() * 2.0;
        [
            (entry(2, 1) - entry(1, 2)) / norm,
            (entry(0, 2) - entry(2, 0)) / norm,
            (entry(1, 0) - entry(0, 1)) / norm,
            norm / 4.0,
        ]
    } else if entry(0, 0) > entry(1, 1) && entry(0, 0) > entry(2, 2) {
        let norm = (1.0 + entry(0, 0) - entry(1, 1) - entry(2, 2)).sqrt() * 2.0;
        [
            norm / 4.0,
            (entry(0, 1) + entry(1, 0)) / norm,
            (entry(0, 2) + entry(2, 0)) / norm,
            (entry(2, 1) - entry(1, 2)) / norm,
        ]
    } else if entry(1, 1) > entry(2, 2) {
        let norm = (1.0 + entry(1, 1) - entry(0, 0) - entry(2, 2)).sqrt() * 2.0;
        [
            (entry(0, 1) + entry(1, 0)) / norm,
            norm / 4.0,
            (entry(1, 2) + entry(2, 1)) / norm,
            (entry(0, 2) - entry(2, 0)) / norm,
        ]
    } else {
        let norm = (1.0 + entry(2, 2) - entry(0, 0) - entry(1, 1)).sqrt() * 2.0;
        [
            (entry(0, 2) + entry(2, 0)) / norm,
            (entry(1, 2) + entry(2, 1)) / norm,
            norm / 4.0,
            (entry(1, 0) - entry(0, 1)) / norm,
        ]
    };
    let sign = if parts[3] < 0.0 { -1.0 } else { 1.0 };
    let micro = |part: f64| round_to_i64(part * sign * MICRO).ok_or(TransformError::OutOfRange);
    Ok(SceneRotation {
        x: micro(parts[0])?,
        y: micro(parts[1])?,
        z: micro(parts[2])?,
        w: micro(parts[3])?,
    })
}

/// Why a camera cannot be shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CameraError {
    #[error("the viewer draws a perspective camera and this one is orthographic")]
    Orthographic,
    #[error("the camera holds a value beyond the scene's range")]
    OutOfRange,
}

/// The orbit the player window's navigation keeps within: from 100 mm to a million metres away,
/// looking up or down at most 1.45 radians.
#[must_use]
pub fn orbit_limits() -> OrbitLimits {
    OrbitLimits {
        min_distance_millimeters: 100.0,
        max_distance_millimeters: 1.0e9,
        min_elevation_radians: -1.45,
        max_elevation_radians: 1.45,
    }
}

/// The scene pose and lens of a presented camera. The scene's orbit needs a point to turn about,
/// which a camera does not have: it is where the view meets the ground plane at height zero when
/// the camera looks down at it from above, otherwise ten metres ahead. A view steeper than the
/// orbit allows is centred further along the horizontal, so the orbit accepts the pose.
pub fn scene_camera(state: &CameraState) -> Result<(CameraPose, SceneCamera), CameraError> {
    let CameraLens::Perspective {
        vertical_fov_radians,
        near_meters,
        far_meters,
    } = state.lens
    else {
        return Err(CameraError::Orthographic);
    };
    let eye = state.pose.eye_meters;
    let eye = [eye.x, eye.y, eye.z];
    let forward = state.pose.forward;
    let forward = [forward.x, forward.y, forward.z];
    let norm = length(forward);
    if !(norm.is_finite() && norm > MIN_SCALE) || eye.iter().any(|part| !part.is_finite()) {
        return Err(CameraError::OutOfRange);
    }
    let target = view_target(eye, forward.map(|part| part / norm));
    let millimeters = |point: Vec3| {
        let mut out = [0; 3];
        for (slot, part) in out.iter_mut().zip(point) {
            *slot = round_to_i64(part * MILLIMETERS_PER_METER).ok_or(CameraError::OutOfRange)?;
        }
        Ok::<_, CameraError>(out)
    };
    let pose = CameraPose {
        eye_millimeters: millimeters(eye)?,
        target_millimeters: millimeters(target)?,
    };
    Ok((
        pose,
        scene_lens(vertical_fov_radians, near_meters, far_meters)?,
    ))
}

/// The point the orbit centres on for an eye looking along the unit vector `forward`.
fn view_target(eye: Vec3, forward: Vec3) -> Vec3 {
    let ground_hit = (forward[2] < -0.01 && eye[2] > 0.0).then(|| eye[2] / -forward[2]);
    let distance = ground_hit
        .unwrap_or(LEVEL_TARGET_METERS)
        .clamp(MIN_TARGET_METERS, MAX_TARGET_METERS);
    let mut target = [0, 1, 2].map(|axis| eye[axis] + forward[axis] * distance);
    let rise = (target[2] - eye[2]).abs();
    let reach = (target[0] - eye[0]).hypot(target[1] - eye[1]);
    let needed = rise / MAX_VIEW_ELEVATION.tan();
    if reach < needed {
        let heading = if reach > MIN_SCALE {
            [(target[0] - eye[0]) / reach, (target[1] - eye[1]) / reach]
        } else {
            [0.0, 1.0]
        };
        target[0] = eye[0] + heading[0] * needed;
        target[1] = eye[1] + heading[1] * needed;
    }
    target
}

fn scene_lens(fov_radians: f64, near: f64, far: f64) -> Result<SceneCamera, CameraError> {
    let whole = |value: f64, low: i64, high: i64| {
        round_to_i64(value)
            .map(|rounded| rounded.clamp(low, high))
            .and_then(|clamped| u32::try_from(clamped).ok())
            .ok_or(CameraError::OutOfRange)
    };
    let widest = i64::from(u32::MAX);
    let near_plane = whole(near * MILLIMETERS_PER_METER, 1, widest - 1)?;
    let far_plane = whole(far * MILLIMETERS_PER_METER, 1, widest)?.max(near_plane + 1);
    Ok(SceneCamera {
        vertical_fov_millidegrees: whole(fov_radians.to_degrees() * 1_000.0, 1, 179_999)?,
        near_plane_millimeters: near_plane,
        far_plane_millimeters: far_plane,
    })
}

#[cfg(test)]
#[path = "scene_transform_tests.rs"]
mod tests;
