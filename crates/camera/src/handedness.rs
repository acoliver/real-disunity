use glam::{DVec3, Mat4, Vec3};

/// Which way a camera's screen-right axis points relative to its forward and up vectors.
///
/// The render basis is the same for both values: only the screen-right sign, and with it the
/// view-depth sign and the compensating projection differ. The authored profile selects
/// the convention; neither convention implies a source-specific world model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ViewHandedness {
    RightHanded,
    DirectXLeftHanded,
}

/// Lens and clip parameters shared by both handedness projections.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lens {
    pub vertical_fov_radians: f64,
    pub aspect: f64,
    pub near: f64,
    pub far: f64,
}

impl ViewHandedness {
    /// `+1.0` when screen-right is `forward x up`, `-1.0` when it is `up x forward`.
    ///
    /// Multiplying a right-handed screen-relative quantity by this sign makes it
    /// display-relative: a yaw increase turns the view toward `-X` at yaw 0, which is
    /// screen-left in a right-handed view and screen-right in a left-handed one.
    pub(crate) fn screen_sign(self) -> f64 {
        match self {
            Self::RightHanded => 1.0,
            Self::DirectXLeftHanded => -1.0,
        }
    }

    /// The matrix that turns a right-handed projection into the left-handed view's visible
    /// raster while the view matrix stays right-handed.
    ///
    /// A left-handed view-projection equals a right-handed one with clip X negated, so screen-space
    /// passes that reconstruct positions from a right-handed affine view keep working when the
    /// projection they receive is this mirrored one. The right-handed value is the identity.
    #[must_use]
    pub fn reconstruction_projection(self, right_handed_projection: Mat4) -> Mat4 {
        match self {
            Self::RightHanded => right_handed_projection,
            Self::DirectXLeftHanded => {
                Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * right_handed_projection
            }
        }
    }

    /// Builds the view-projection for an eye looking at `target` with the given vertical FOV.
    pub(crate) fn view_projection(self, eye: DVec3, target: DVec3, lens: Lens) -> Mat4 {
        let Lens {
            vertical_fov_radians,
            aspect,
            near,
            far,
        } = lens;
        match self {
            Self::RightHanded => {
                let view = glam::dcamera::rh::view::look_at_mat4(eye, target, DVec3::Y);
                let projection = glam::dcamera::rh::proj::directx::perspective(
                    vertical_fov_radians,
                    aspect,
                    near,
                    far,
                );
                (projection * view).as_mat4()
            }
            Self::DirectXLeftHanded => {
                let view = glam::dcamera::lh::view::look_at_mat4(eye, target, DVec3::Y);
                let projection = glam::dcamera::lh::proj::directx::perspective(
                    vertical_fov_radians,
                    aspect,
                    near,
                    far,
                );
                (projection * view).as_mat4()
            }
        }
    }
}
