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

use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};
use thiserror::Error;
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
    /// The eye and target coincide, so the view direction is undefined.
    #[error("camera eye and target coincide, leaving the view direction undefined")]
    DegenerateDirection,
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

/// The resolved view and projection for one camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneView {
    view: Mat4,
    projection: Mat4,
    eye_meters: Vec3,
    eye_millimeters: [i64; 3],
    viewport: Viewport,
    clip_distances_meters: [f32; 2],
}

impl SceneView {
    /// Resolves a pose and a validated camera into view and projection.
    ///
    /// The projection cannot be degenerate: [`SceneCamera`] is validated by the
    /// renderer domain to hold a field of view in `1..=179_999` millidegrees
    /// and a far plane strictly beyond a positive near plane. Only the pose and
    /// the viewport, which arrive from outside the scene, can fail here.
    ///
    /// # Errors
    ///
    /// Returns [`ViewError`] when the pose does not determine a view basis or
    /// the viewport has no area.
    pub fn new(
        pose: CameraPose,
        camera: SceneCamera,
        viewport: Viewport,
    ) -> Result<Self, ViewError> {
        if viewport.width == 0 || viewport.height == 0 {
            return Err(ViewError::DegenerateViewport {
                width: viewport.width,
                height: viewport.height,
            });
        }
        let eye = meters(pose.eye_millimeters);
        let target = meters(pose.target_millimeters);
        let forward = target - eye;
        if forward.length_squared() == 0.0 {
            return Err(ViewError::DegenerateDirection);
        }
        let forward = forward.normalize();
        // The right vector is what carries the Z-up world into a Y-up view. It
        // collapses exactly when the view direction is parallel to world up.
        let right = forward.cross(SCENE_UP);
        if right.length_squared() == 0.0 {
            return Err(ViewError::DegenerateRoll);
        }

        // `look_at_rh` builds a right-handed view whose space is +X right,
        // +Y up, -Z forward. Handing it a Z-up world and a Z up axis performs
        // the Z-up-to-Y-up change here and nowhere else, as a rotation rather
        // than an axis swap, so handedness — and therefore face winding — is
        // preserved.
        let view = look_at_mat4(eye, target, SCENE_UP);

        let aspect = f64_to_f32(f64::from(viewport.width) / f64::from(viewport.height));
        let projection = perspective(
            radians(camera.vertical_fov_millidegrees),
            aspect,
            meters_scalar(camera.near_plane_millimeters),
            meters_scalar(camera.far_plane_millimeters),
        );
        Ok(Self {
            view,
            projection,
            eye_meters: eye,
            eye_millimeters: pose.eye_millimeters,
            viewport,
            clip_distances_meters: [
                meters_scalar(camera.near_plane_millimeters),
                meters_scalar(camera.far_plane_millimeters),
            ],
        })
    }

    /// Exact near and far distances used to construct this projection, in metres.
    #[must_use]
    pub const fn clip_distances_meters(&self) -> [f32; 2] {
        self.clip_distances_meters
    }

    /// Returns the viewport used to resolve this projection.
    #[must_use]
    pub const fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Returns the world-to-view matrix, the sole Z-up to Y-up conversion.
    #[must_use]
    pub const fn view_matrix(&self) -> Mat4 {
        self.view
    }

    /// Returns the view-to-clip matrix for reversed-none `0..=1` depth.
    #[must_use]
    pub const fn projection_matrix(&self) -> Mat4 {
        self.projection
    }

    /// Returns the combined world-to-clip matrix.
    #[must_use]
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection * self.view
    }

    /// Returns the eye position in scene-space metres.
    ///
    /// Shading needs a view vector in the same space as the geometry, which is
    /// still Z-up: the basis change lives only in [`Self::view_matrix`].
    #[must_use]
    pub const fn eye_meters(&self) -> Vec3 {
        self.eye_meters
    }

    /// Returns the authored eye position for CPU distance selection.
    #[must_use]
    pub const fn eye_millimeters(&self) -> [i64; 3] {
        self.eye_millimeters
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
pub(crate) fn meters_scalar(millimeters: u32) -> f32 {
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
