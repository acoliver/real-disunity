use glam::{DMat4, DVec3, Mat4};

use super::{CameraLens, CameraState, ResolvedView, ViewError};
use crate::ViewHandedness;

pub(super) fn resolve(state: &CameraState, aspect: f64) -> Result<ResolvedView, ViewError> {
    let pose = state.pose;
    if !pose.eye_meters.is_finite() || !pose.forward.is_finite() || !pose.up.is_finite() {
        return Err(ViewError::NonFinitePose);
    }
    let forward = pose
        .forward
        .try_normalize()
        .ok_or(ViewError::DegenerateDirection)?;
    let up = pose.up.try_normalize().ok_or(ViewError::DegenerateRoll)?;
    if forward.cross(up).try_normalize().is_none() {
        return Err(ViewError::DegenerateRoll);
    }
    let reconstruction_view = glam::dcamera::rh::view::look_to_mat4(pose.eye_meters, forward, up);
    let right_projection = projection(state.lens, aspect)?;
    let (view, projection) = match state.convention.handedness {
        ViewHandedness::RightHanded => (reconstruction_view, right_projection),
        ViewHandedness::DirectXLeftHanded => (
            DMat4::from_scale(DVec3::new(-1.0, 1.0, -1.0)) * reconstruction_view,
            right_projection * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
        ),
    };
    let query_inverse = (projection * view).inverse();
    validate_clip_inverse(query_inverse)?;
    validate_clip_inverse(projection.inverse())?;
    let view_projection = (projection * view).as_mat4();
    let inverse_view_projection = inverse(view_projection)?;
    let view = view.as_mat4();
    let projection = projection.as_mat4();
    let reconstruction_view = reconstruction_view.as_mat4();
    let reconstruction_projection = state
        .convention
        .handedness
        .reconstruction_projection(right_projection.as_mat4());
    for matrix in [
        view,
        projection,
        reconstruction_view,
        reconstruction_projection,
    ] {
        inverse(matrix)?;
    }
    let eye_meters = pose.eye_meters.as_vec3();
    if !eye_meters.is_finite() {
        return Err(ViewError::SingularTransform);
    }
    Ok(ResolvedView {
        view,
        projection,
        view_projection,
        inverse_view_projection,
        query_inverse,
        reconstruction_view,
        reconstruction_projection,
        eye_meters,
        clip_distances_meters: clip_distances(state.lens),
        viewport: state.viewport,
        convention: state.convention,
    })
}

fn inverse(matrix: Mat4) -> Result<Mat4, ViewError> {
    let determinant = matrix.determinant();
    if !matrix.is_finite() || !determinant.is_finite() || determinant == 0.0 {
        return Err(ViewError::SingularTransform);
    }
    let inverse = matrix.inverse();
    if !inverse.is_finite() {
        return Err(ViewError::SingularTransform);
    }
    Ok(inverse)
}

fn projection(lens: CameraLens, aspect: f64) -> Result<DMat4, ViewError> {
    use glam::dcamera::rh::proj::directx::{orthographic, perspective};
    match lens {
        CameraLens::Perspective {
            vertical_fov_radians: fov,
            near_meters: near,
            far_meters: far,
        } => {
            if !fov.is_finite()
                || fov <= 0.0
                || fov >= std::f64::consts::PI
                || !valid_clip(near, far)
                || near == 0.0
            {
                return Err(ViewError::InvalidPerspectiveLens);
            }
            Ok(perspective(fov, aspect, near, far))
        }
        CameraLens::Orthographic {
            half_height_meters: half,
            near_meters: near,
            far_meters: far,
        } => {
            if !half.is_finite() || half <= 0.0 || !valid_clip(near, far) {
                return Err(ViewError::InvalidOrthographicLens);
            }
            let width = half * aspect;
            if !width.is_finite() {
                return Err(ViewError::SingularTransform);
            }
            Ok(orthographic(-width, width, -half, half, near, far))
        }
    }
}

fn valid_clip(near: f64, far: f64) -> bool {
    near.is_finite() && near >= 0.0 && far.is_finite() && far > near
}

fn validate_clip_inverse(inverse: DMat4) -> Result<(), ViewError> {
    for depth in [0.0, 1.0] {
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                let point = inverse * glam::DVec4::new(x, y, depth, 1.0);
                if point.w == 0.0 || !(point.truncate() / point.w).as_vec3().is_finite() {
                    return Err(ViewError::SingularTransform);
                }
            }
        }
    }
    Ok(())
}

fn clip_distances(lens: CameraLens) -> [f32; 2] {
    let (CameraLens::Perspective {
        near_meters,
        far_meters,
        ..
    }
    | CameraLens::Orthographic {
        near_meters,
        far_meters,
        ..
    }) = lens;
    glam::DVec2::new(near_meters, far_meters)
        .as_vec2()
        .to_array()
}
