//! CPU camera contract shared by controllers, rasterization and screen queries.
//!
//! Inputs are in the caller's canonical metre space. No unit conversion or world-axis
//! swizzle occurs here. Matrices multiply column vectors, use non-reversed 0..1 depth,
//! and screen queries use top-left pixel coordinates at continuous pixel edges.

use glam::{DVec3, Mat4, Vec2, Vec3};

use crate::{ViewConvention, Viewport};

mod matrices;

/// Eye and explicit forward/up directions. The resolver orthonormalizes the basis.
/// A planar camera must supply an up direction independent of its forward direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPose {
    pub eye_meters: DVec3,
    pub forward: DVec3,
    pub up: DVec3,
}

/// Lens dimensions in metres and radians. Orthographic size is half the vertical span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraLens {
    Perspective {
        vertical_fov_radians: f64,
        near_meters: f64,
        far_meters: f64,
    },
    Orthographic {
        half_height_meters: f64,
        near_meters: f64,
        far_meters: f64,
    },
}

/// Complete inputs for one immutable resolved camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    pub pose: CameraPose,
    pub lens: CameraLens,
    pub viewport: Viewport,
    pub convention: ViewConvention,
}

/// Raster depth after homogeneous division.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthConvention {
    /// Near is zero and far is one; depth is not reversed.
    ZeroToOne,
}

/// Invalid camera inputs, unrepresentable transforms or invalid screen queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewError {
    DegenerateViewport { width: u32, height: u32 },
    NonFinitePose,
    DegenerateDirection,
    DegenerateRoll,
    InvalidPerspectiveLens,
    InvalidOrthographicLens,
    SingularTransform,
    InvalidWorldPoint,
    InvalidScreenPoint,
    PointAtInfinity,
    ParallelRay,
    IntersectionBehindRay,
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DegenerateViewport { width, height } => {
                write!(f, "viewport {width}x{height} has a zero dimension")
            }
            Self::NonFinitePose => f.write_str("camera pose must be finite"),
            Self::DegenerateDirection => f.write_str("camera forward direction is undefined"),
            Self::DegenerateRoll => f.write_str("camera up direction does not constrain roll"),
            Self::InvalidPerspectiveLens => {
                f.write_str("perspective lens requires 0 < fov < pi and 0 < near < far")
            }
            Self::InvalidOrthographicLens => {
                f.write_str("orthographic lens requires positive half height and 0 <= near < far")
            }
            Self::SingularTransform => f.write_str(
                "camera transform or inverse is not representable as a finite GPU matrix",
            ),
            Self::InvalidWorldPoint => f.write_str("world point must be finite"),
            Self::InvalidScreenPoint => {
                f.write_str("screen point must be finite with depth in 0..=1")
            }
            Self::PointAtInfinity => f.write_str("point has no finite homogeneous projection"),
            Self::ParallelRay => f.write_str("ray is parallel to the plane"),
            Self::IntersectionBehindRay => {
                f.write_str("plane intersection is behind the ray origin")
            }
        }
    }
}

impl std::error::Error for ViewError {}

/// Coupled raster matrices, inverse and right-handed reconstruction matrices.
///
/// Reconstruction always uses a right-handed affine view. For the existing
/// DirectX-left-handed convention its projection mirrors clip X, preserving the
/// visible raster. Consumers must pair the reconstruction matrices from this value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedView {
    view: Mat4,
    projection: Mat4,
    view_projection: Mat4,
    inverse_view_projection: Mat4,
    reconstruction_view: Mat4,
    reconstruction_projection: Mat4,
    eye_meters: Vec3,
    viewport: Viewport,
    convention: ViewConvention,
}

impl ResolvedView {
    pub const fn view_matrix(&self) -> Mat4 {
        self.view
    }
    pub const fn projection_matrix(&self) -> Mat4 {
        self.projection
    }
    pub const fn view_projection_matrix(&self) -> Mat4 {
        self.view_projection
    }
    pub const fn inverse_view_projection_matrix(&self) -> Mat4 {
        self.inverse_view_projection
    }
    pub const fn reconstruction_view_matrix(&self) -> Mat4 {
        self.reconstruction_view
    }
    pub const fn reconstruction_projection_matrix(&self) -> Mat4 {
        self.reconstruction_projection
    }
    pub const fn eye_meters(&self) -> Vec3 {
        self.eye_meters
    }
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }
    pub const fn convention(&self) -> ViewConvention {
        self.convention
    }
    pub const fn depth_convention(&self) -> DepthConvention {
        DepthConvention::ZeroToOne
    }

    /// Projects to continuous top-left pixel coordinates and 0..1 raster depth.
    /// Points outside the frustum are permitted; use `contains_point` for clipping.
    pub fn world_to_screen(&self, point: Vec3) -> Result<Vec3, ViewError> {
        if !point.is_finite() {
            return Err(ViewError::InvalidWorldPoint);
        }
        let ndc = homogeneous_point(self.view_projection, point)?;
        let size = self.viewport_size();
        let screen = Vec3::new(
            (ndc.x + 1.0) * 0.5 * size.x,
            (1.0 - ndc.y) * 0.5 * size.y,
            ndc.z,
        );
        if !screen.is_finite() {
            return Err(ViewError::PointAtInfinity);
        }
        Ok(screen)
    }

    /// Unprojects a continuous screen coordinate and non-reversed raster depth.
    pub fn screen_to_world(&self, point: Vec3) -> Result<Vec3, ViewError> {
        if !point.is_finite() || !(0.0..=1.0).contains(&point.z) {
            return Err(ViewError::InvalidScreenPoint);
        }
        let size = self.viewport_size();
        let ndc = Vec3::new(
            2.0 * point.x / size.x - 1.0,
            1.0 - 2.0 * point.y / size.y,
            point.z,
        );
        homogeneous_point(self.inverse_view_projection, ndc)
    }

    /// Tests all six homogeneous clip planes, rejecting nonfinite points.
    pub fn contains_point(&self, point: Vec3) -> bool {
        let clip = self.view_projection * point.extend(1.0);
        clip.is_finite()
            && clip.w > 0.0
            && clip.x.abs() <= clip.w
            && clip.y.abs() <= clip.w
            && clip.z >= 0.0
            && clip.z <= clip.w
    }

    /// Builds a forward ray originating on the near plane for either lens family.
    pub fn screen_ray(&self, pixel: Vec2) -> Result<ScreenRay, ViewError> {
        let origin_meters = self.screen_to_world(pixel.extend(0.0))?;
        let far = self.screen_to_world(pixel.extend(1.0))?;
        let direction = (far.as_dvec3() - origin_meters.as_dvec3())
            .try_normalize()
            .ok_or(ViewError::PointAtInfinity)?
            .as_vec3();
        Ok(ScreenRay {
            origin_meters,
            direction,
        })
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "pixel queries use the same f32 precision as GPU rasterization"
    )]
    fn viewport_size(&self) -> Vec2 {
        Vec2::new(self.viewport.width as f32, self.viewport.height as f32)
    }
}

/// A unit world-space ray whose origin lies on the resolved near plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRay {
    pub origin_meters: Vec3,
    pub direction: Vec3,
}

impl ScreenRay {
    /// Intersects the forward half-ray with a plane specified in canonical metres.
    pub fn intersect_plane(&self, point: Vec3, normal: Vec3) -> Result<Vec3, ViewError> {
        if !point.is_finite()
            || !normal.is_finite()
            || !self.origin_meters.is_finite()
            || !self.direction.is_finite()
        {
            return Err(ViewError::InvalidWorldPoint);
        }
        let normal = normal
            .as_dvec3()
            .try_normalize()
            .ok_or(ViewError::ParallelRay)?;
        let direction = self.direction.as_dvec3();
        let origin = self.origin_meters.as_dvec3();
        let denominator = direction.dot(normal);
        if denominator == 0.0 {
            return Err(ViewError::ParallelRay);
        }
        let distance = (point.as_dvec3() - origin).dot(normal) / denominator;
        if distance < 0.0 {
            return Err(ViewError::IntersectionBehindRay);
        }
        let intersection = (origin + direction * distance).as_vec3();
        if !intersection.is_finite() {
            return Err(ViewError::PointAtInfinity);
        }
        Ok(intersection)
    }
}

fn homogeneous_point(matrix: Mat4, point: Vec3) -> Result<Vec3, ViewError> {
    let clip = matrix * point.extend(1.0);
    if !clip.is_finite() || clip.w == 0.0 {
        return Err(ViewError::PointAtInfinity);
    }
    let point = clip.truncate() / clip.w;
    if !point.is_finite() {
        return Err(ViewError::PointAtInfinity);
    }
    Ok(point)
}

/// Resolves one validated, finite and invertible camera without changing world basis.
pub fn resolve(state: &CameraState) -> Result<ResolvedView, ViewError> {
    let aspect = f64::from(state.viewport.width) / f64::from(state.viewport.height);
    resolve_with_aspect(state, aspect)
}

pub(crate) fn resolve_with_aspect(
    state: &CameraState,
    aspect: f64,
) -> Result<ResolvedView, ViewError> {
    if state.viewport.width == 0 || state.viewport.height == 0 {
        return Err(ViewError::DegenerateViewport {
            width: state.viewport.width,
            height: state.viewport.height,
        });
    }
    matrices::resolve(state, aspect)
}
