//! Camera pose and projection for a governed scene.
//!
//! # The single basis change
//!
//! Scene space is Z-up: the authored world places `+Z` against gravity. Clip
//! space is Y-up. Somewhere between the two a basis change must happen, and the
//! failure mode this module exists to prevent is applying it more than once, or
//! in more than one place.
//!
//! Distributing the change across the pipeline — swizzling vertices on upload,
//! or transposing object transforms, or flipping normals in a shader — is the
//! usual way this goes wrong, because each site looks locally correct while the
//! composition is not. It also silently inverts handedness whenever a site
//! swaps two axes instead of rotating, which turns back-face culling into
//! front-face culling and mirrors every normal map.
//!
//! So this module holds the only conversion in the renderer. Mesh vertices,
//! object transforms, light directions and terrain all stay in authored Z-up
//! scene space, in millimetres, exactly as the domain expresses them.
//! [`SceneView::view_matrix`] performs the change once, by constructing the
//! view basis directly from a Z-up world. No other code is permitted to
//! reinterpret an axis.

use glam::{Mat4, Vec3};
use thiserror::Error;
use worldrunner_camera::{
    CameraLens, CameraState, ResolvedView, ViewConvention, ViewHandedness, resolve,
};
use worldrunner_render_plan::SceneCamera;

/// Millimetres per metre; scene distances are integral millimetres.
const MILLIMETERS_PER_METER: f32 = 1_000.0;

/// Millidegrees per degree; scene angles are integral millidegrees.
const MILLIDEGREES_PER_DEGREE: f32 = 1_000.0;

/// The authored world up axis. Scene space is Z-up by construction.
const SCENE_UP: Vec3 = Vec3::Z;

/// A camera pose could not describe a view.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ViewError {
    /// The shared resolver rejected a lens or an unrepresentable GPU transform.
    #[error("camera resolution failed: {0}")]
    Resolution(worldrunner_camera::ViewError),
    /// The eye and target coincide, so the view direction is undefined.
    #[error("camera eye and target coincide, leaving the view direction undefined")]
    DegenerateDirection,
    /// The lens cannot describe a finite orthographic volume.
    #[error("orthographic lens requires a finite positive half extent and 0 <= near < far")]
    InvalidOrthographicLens,
    /// The view direction is parallel to the world up axis.
    ///
    /// A camera looking straight down cannot derive a right vector from the up
    /// axis, so the roll about the view direction is unconstrained. Rejecting
    /// this is honest; picking an arbitrary roll would make the captured image
    /// depend on an unstated convention.
    #[error("camera view direction is parallel to the world up axis, leaving roll unconstrained")]
    DegenerateRoll,
    /// The viewport had a zero dimension, so the aspect ratio is undefined.
    #[error("viewport {width}x{height} has a zero dimension")]
    DegenerateViewport {
        /// The requested viewport width in pixels.
        width: u32,
        /// The requested viewport height in pixels.
        height: u32,
    },
}

/// Where the camera sits and what it looks at, in authored scene space.
///
/// The pose is not part of [`RenderScene`] because no authored scene declares
/// one; it is supplied by whatever holds camera authority. It is therefore the
/// only camera input this renderer cannot assume is already validated.
///
/// [`RenderScene`]: worldrunner_render_plan::RenderScene
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraPose {
    /// The eye position in scene millimetres.
    pub eye_millimeters: [i64; 3],
    /// The point the eye looks at, in scene millimetres.
    pub target_millimeters: [i64; 3],
}

/// A viewport size in whole pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    /// Width in pixels; strictly positive.
    pub width: u32,
    /// Height in pixels; strictly positive.
    pub height: u32,
}

/// An orthographic lens in canonical scene metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrthographicLens {
    /// Half the visible vertical extent; horizontal extent follows the viewport aspect.
    pub vertical_half_extent_meters: f32,
    /// Forward distance to the near plane, including zero.
    pub near_meters: f32,
    /// Forward distance to the far plane.
    pub far_meters: f32,
}

/// The resolved view and projection for one camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneView {
    resolved: ResolvedView,
}

impl SceneView {
    /// Borrows the same CPU camera used by rasterization and screen queries.
    #[must_use]
    pub const fn resolved_view(&self) -> &ResolvedView {
        &self.resolved
    }

    /// Adapts an already resolved CPU camera without recomputing matrices.
    #[must_use]
    pub const fn from_resolved(resolved: ResolvedView) -> Self {
        Self { resolved }
    }

    /// Resolves an orthographic lens using the same canonical Z-up view basis.
    pub fn orthographic(
        pose: CameraPose,
        lens: OrthographicLens,
        viewport: Viewport,
    ) -> Result<Self, ViewError> {
        Self::orthographic_with_up(pose, lens, SCENE_UP.to_array(), viewport)
    }

    /// Resolves an orthographic camera with an explicit canonical up direction for roll.
    pub fn orthographic_with_up(
        pose: CameraPose,
        lens: OrthographicLens,
        up: [f32; 3],
        viewport: Viewport,
    ) -> Result<Self, ViewError> {
        let lens = CameraLens::Orthographic {
            half_height_meters: f64::from(lens.vertical_half_extent_meters),
            near_meters: f64::from(lens.near_meters),
            far_meters: f64::from(lens.far_meters),
        };
        resolve_scene(pose, lens, Vec3::from_array(up), viewport)
    }

    /// Resolves an integral governed camera through the shared CPU contract.
    pub fn new(
        pose: CameraPose,
        camera: SceneCamera,
        viewport: Viewport,
    ) -> Result<Self, ViewError> {
        let lens = CameraLens::Perspective {
            vertical_fov_radians: f64::from(radians(camera.vertical_fov_millidegrees)),
            near_meters: f64::from(meters_scalar(camera.near_plane_millimeters)),
            far_meters: f64::from(meters_scalar(camera.far_plane_millimeters)),
        };
        resolve_scene(pose, lens, SCENE_UP, viewport)
    }

    /// Returns the viewport used to resolve this projection.
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        let viewport = self.resolved.viewport();
        Viewport {
            width: viewport.width,
            height: viewport.height,
        }
    }

    /// Returns the world-to-view matrix, the sole Z-up to Y-up conversion.
    #[must_use]
    pub const fn view_matrix(&self) -> Mat4 {
        self.resolved.view_matrix()
    }

    /// Returns the view-to-clip matrix for non-reversed `0..=1` depth.
    #[must_use]
    pub const fn projection_matrix(&self) -> Mat4 {
        self.resolved.projection_matrix()
    }

    /// Returns the combined world-to-clip matrix.
    #[must_use]
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.resolved.view_projection_matrix()
    }

    /// Returns the eye position in unchanged scene-space metres for shading.
    #[must_use]
    pub const fn eye_meters(&self) -> Vec3 {
        self.resolved.eye_meters()
    }
}

fn resolve_scene(
    pose: CameraPose,
    lens: CameraLens,
    up: Vec3,
    viewport: Viewport,
) -> Result<SceneView, ViewError> {
    let eye = meters(pose.eye_millimeters);
    let target = meters(pose.target_millimeters);
    let state = CameraState {
        pose: worldrunner_camera::CameraPose {
            eye_meters: eye.as_dvec3(),
            forward: (target - eye).as_dvec3(),
            up: up.as_dvec3(),
        },
        lens,
        viewport: worldrunner_camera::Viewport {
            width: viewport.width,
            height: viewport.height,
        },
        convention: ViewConvention {
            handedness: ViewHandedness::RightHanded,
        },
    };
    resolve(&state)
        .map(SceneView::from_resolved)
        .map_err(scene_error)
}

fn scene_error(error: worldrunner_camera::ViewError) -> ViewError {
    use worldrunner_camera::ViewError as Shared;
    match error {
        Shared::DegenerateViewport { width, height } => {
            ViewError::DegenerateViewport { width, height }
        }
        Shared::DegenerateDirection => ViewError::DegenerateDirection,
        Shared::DegenerateRoll | Shared::NonFinitePose => ViewError::DegenerateRoll,
        Shared::InvalidOrthographicLens => ViewError::InvalidOrthographicLens,
        other => ViewError::Resolution(other),
    }
}

/// Rewrites integral scene millimetres into metres.
///
/// Metres rather than millimetres because a 32-bit float holds about seven
/// significant digits: a world coordinate expressed in millimetres loses
/// sub-millimetre exactness beyond roughly 16 kilometres from the origin, and
/// millimetre-scale jitter in a vertex position is visible as surface crawl.
#[expect(
    clippy::cast_precision_loss,
    reason = "scene coordinates are metre-scale once divided, far inside the exactly representable \
              integer range of f32, and the GPU pipeline is single precision regardless"
)]
fn meters(millimeters: [i64; 3]) -> Vec3 {
    Vec3::new(
        millimeters[0] as f32 / MILLIMETERS_PER_METER,
        millimeters[1] as f32 / MILLIMETERS_PER_METER,
        millimeters[2] as f32 / MILLIMETERS_PER_METER,
    )
}

/// Rewrites one integral millimetre distance into metres.
fn meters_scalar(millimeters: u32) -> f32 {
    f64_to_f32(f64::from(millimeters) / f64::from(MILLIMETERS_PER_METER))
}

/// Rewrites integral millidegrees into radians.
fn radians(millidegrees: u32) -> f32 {
    f64_to_f32((f64::from(millidegrees) / f64::from(MILLIDEGREES_PER_DEGREE)).to_radians())
}

/// Narrows a double to the single precision the GPU pipeline uses.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is a metre-scale distance or a radian angle, both far inside f32 range, and \
              the graphics pipeline is single precision"
)]
fn f64_to_f32(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests;
