//! Controller lens policies adapted to the common resolved camera.

use crate::{
    CameraLens, CameraPose, CameraState, CameraView, FreeFlyCamera, MAX_FOV_RADIANS,
    MIN_FOV_RADIANS, MIN_NEAR_PLANE_METERS, ResolvedView, RtsCamera, ViewConvention, ViewError,
    Viewport, finite_plane, resolved,
};
use glam::{DVec3, Mat4};

impl FreeFlyCamera {
    /// Resolves this controller through the shared CPU camera contract.
    pub fn resolved_view(
        self,
        viewport: Viewport,
        near: f64,
        far: f64,
    ) -> Result<ResolvedView, ViewError> {
        let aspect = f64::from(viewport.width) / f64::from(viewport.height);
        self.resolved_for_aspect(viewport, aspect, near, far)
    }

    pub fn camera_view(
        self,
        aspect: f64,
        near_plane_meters: f64,
        far_plane_meters: f64,
    ) -> CameraView {
        let position = self.position.as_vec3();
        let result = self.resolved_for_aspect(
            Viewport {
                width: 1,
                height: 1,
            },
            aspect,
            near_plane_meters,
            far_plane_meters,
        );
        CameraView {
            view_projection: result.map_or(Mat4::NAN, |view| view.view_projection_matrix()),
            position,
        }
    }

    fn resolved_for_aspect(
        self,
        viewport: Viewport,
        aspect: f64,
        near_plane_meters: f64,
        far_plane_meters: f64,
    ) -> Result<ResolvedView, ViewError> {
        let aspect = if aspect.is_finite() {
            aspect.max(0.01)
        } else {
            1.0
        };
        let near = if near_plane_meters.is_finite() {
            near_plane_meters.max(MIN_NEAR_PLANE_METERS)
        } else {
            0.5
        };
        let far = if far_plane_meters.is_finite() {
            far_plane_meters.max(near + 1.0)
        } else {
            near + 1.0
        };
        resolved::resolve_with_aspect(
            &CameraState {
                pose: CameraPose {
                    eye_meters: self.position,
                    forward: self.forward(),
                    up: DVec3::Y,
                },
                lens: CameraLens::Perspective {
                    vertical_fov_radians: self
                        .vertical_fov_radians
                        .clamp(MIN_FOV_RADIANS, MAX_FOV_RADIANS),
                    near_meters: near,
                    far_meters: far,
                },
                viewport,
                convention: ViewConvention {
                    handedness: self.handedness,
                },
            },
            aspect,
        )
    }
}

impl RtsCamera {
    /// Resolves this controller through the shared CPU camera contract.
    pub fn resolved_view(
        self,
        viewport: Viewport,
        near: f64,
        far: f64,
    ) -> Result<ResolvedView, ViewError> {
        let aspect = f64::from(viewport.width) / f64::from(viewport.height);
        self.resolved_for_aspect(viewport, aspect, near, far)
    }

    pub fn camera_view(
        self,
        aspect: f64,
        near_plane_meters: f64,
        far_plane_meters: f64,
    ) -> CameraView {
        let position = self.eye().as_vec3();
        let result = self.resolved_for_aspect(
            Viewport {
                width: 1,
                height: 1,
            },
            aspect,
            near_plane_meters,
            far_plane_meters,
        );
        CameraView {
            view_projection: result.map_or(Mat4::NAN, |view| view.view_projection_matrix()),
            position,
        }
    }

    fn resolved_for_aspect(
        self,
        viewport: Viewport,
        aspect: f64,
        near_plane_meters: f64,
        far_plane_meters: f64,
    ) -> Result<ResolvedView, ViewError> {
        let aspect = if aspect.is_finite() {
            aspect.max(0.01)
        } else {
            1.0
        };
        let near = finite_plane(near_plane_meters, MIN_NEAR_PLANE_METERS);
        let far = finite_plane(far_plane_meters, near + 1.0).max(near + 1.0);
        let horizontal_fov = self.horizontal_fov_radians_for_aspect(aspect);
        let vertical_fov = 2.0 * ((horizontal_fov * 0.5).tan() / aspect).atan();
        resolved::resolve_with_aspect(
            &CameraState {
                pose: CameraPose {
                    eye_meters: self.eye(),
                    forward: self.look_at() - self.eye(),
                    up: DVec3::Y,
                },
                lens: CameraLens::Perspective {
                    vertical_fov_radians: vertical_fov.clamp(MIN_FOV_RADIANS, MAX_FOV_RADIANS),
                    near_meters: near,
                    far_meters: far,
                },
                viewport,
                convention: ViewConvention {
                    handedness: self.handedness,
                },
            },
            aspect,
        )
    }
}
