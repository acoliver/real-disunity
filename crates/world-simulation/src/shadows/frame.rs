//! Matrices and planes of a shadow map's view volume.

use glam::{Mat4, Vec3, Vec4};

use super::ShadowError;

/// The axes of a light: the light shines along `forward`.
#[derive(Clone, Copy, Debug)]
pub(super) struct LightBasis {
    pub(super) right: Vec3,
    pub(super) up: Vec3,
    pub(super) forward: Vec3,
}

impl LightBasis {
    /// The orthonormal axes of a light's matrix.
    pub(super) fn of(matrix: &Mat4) -> Result<Self, ShadowError> {
        let forward = matrix.z_axis.truncate().try_normalize();
        let x_axis = matrix.x_axis.truncate();
        let forward = forward.ok_or(ShadowError::InvalidLight)?;
        let right = (x_axis - forward * x_axis.dot(forward))
            .try_normalize()
            .ok_or(ShadowError::InvalidLight)?;
        Ok(Self {
            right,
            up: forward.cross(right),
            forward,
        })
    }

    /// The axes of a cube face, the way a point light looks along it. The up axis of the two
    /// vertical faces is along z so that no face looks along its own up.
    pub(super) fn of_face(face: u32) -> Result<Self, ShadowError> {
        let (forward, up) = match face {
            0 => (Vec3::X, Vec3::Y),
            1 => (-Vec3::X, Vec3::Y),
            2 => (Vec3::Y, -Vec3::Z),
            3 => (-Vec3::Y, Vec3::Z),
            4 => (Vec3::Z, Vec3::Y),
            5 => (-Vec3::Z, Vec3::Y),
            _ => return Err(ShadowError::InvalidFace),
        };
        Ok(Self {
            right: up.cross(forward),
            up,
            forward,
        })
    }

    /// The coordinates of a world point along the axes.
    pub(super) fn coordinates_of(&self, point: Vec3) -> Vec3 {
        Vec3::new(
            self.right.dot(point),
            self.up.dot(point),
            self.forward.dot(point),
        )
    }

    /// The world point of coordinates along the axes.
    pub(super) fn point_at(&self, coordinates: Vec3) -> Vec3 {
        self.right * coordinates.x + self.up * coordinates.y + self.forward * coordinates.z
    }

    /// The matrix from the world to a camera at `eye` with these axes, looking down negative z.
    pub(super) fn view(&self, eye: Vec3) -> Mat4 {
        let rows = [self.right, self.up, -self.forward];
        let translation = Vec3::new(-rows[0].dot(eye), -rows[1].dot(eye), -rows[2].dot(eye));
        Mat4::from_cols(
            Vec4::new(rows[0].x, rows[1].x, rows[2].x, 0.0),
            Vec4::new(rows[0].y, rows[1].y, rows[2].y, 0.0),
            Vec4::new(rows[0].z, rows[1].z, rows[2].z, 0.0),
            translation.extend(1.0),
        )
    }
}

/// An orthographic projection of the box `-half..half` in x and y and the distances `near` to
/// `far` in front of the camera, with depth -1 at the near plane and 1 at the far plane.
pub(super) fn orthographic(half: f32, near: f32, far: f32) -> Mat4 {
    let depth = far - near;
    Mat4::from_cols(
        Vec4::new(1.0 / half, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0 / half, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -2.0 / depth, 0.0),
        Vec4::new(0.0, 0.0, -(far + near) / depth, 1.0),
    )
}

/// A perspective projection of a square view with the full angle `fov_degrees`, with depth -1 at
/// the near plane and 1 at the far plane.
pub(super) fn perspective(fov_degrees: f32, near: f32, far: f32) -> Mat4 {
    let scale = 1.0 / (fov_degrees.to_radians() * 0.5).tan();
    let depth = far - near;
    Mat4::from_cols(
        Vec4::new(scale, 0.0, 0.0, 0.0),
        Vec4::new(0.0, scale, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -(far + near) / depth, -1.0),
        Vec4::new(0.0, 0.0, -2.0 * far * near / depth, 0.0),
    )
}

/// The four side planes and the far plane of the volume a projection times view matrix sees,
/// normals into the volume. The near plane is left out: a caster between a light and its near
/// plane still shadows what the light sees.
pub(super) fn side_and_far_planes(matrix: &Mat4) -> Vec<Vec4> {
    let [x, y, z, w] = [0, 1, 2, 3].map(|row| matrix.row(row));
    [w + x, w - x, w + y, w - y, w - z]
        .into_iter()
        .map(|plane| {
            let length = plane.truncate().length();
            plane / length
        })
        .collect()
}
