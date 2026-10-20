//! Directional fit in caller-selected world coordinates, with WebGPU zero-to-one depth.
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct DirectionalFit {
    pub center: Vec3,
    pub toward_light: Vec3,
    pub up: Vec3,
    pub half_extent: f32,
    pub depth_half_extent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("directional fit must be finite, nondegenerate and representable")]
pub struct InvalidFit;

impl DirectionalFit {
    pub fn matrix(self) -> Result<Mat4, InvalidFit> {
        if !self.center.is_finite()
            || !self.half_extent.is_finite()
            || self.half_extent <= 0.0
            || !self.depth_half_extent.is_finite()
            || self.depth_half_extent <= 0.0
        {
            return Err(InvalidFit);
        }
        let direction = self.toward_light.try_normalize().ok_or(InvalidFit)?;
        let up = self.up.try_normalize().ok_or(InvalidFit)?;
        let up = if direction.dot(up).abs() > 0.99 {
            up.any_orthonormal_vector()
        } else {
            up
        };
        let eye = self.center + direction * self.depth_half_extent;
        let view = glam::camera::rh::view::look_at_mat4(eye, self.center, up);
        let e = self.half_extent;
        let projection = glam::camera::rh::proj::directx::orthographic(
            -e,
            e,
            -e,
            e,
            0.0,
            self.depth_half_extent * 2.0,
        );
        let matrix = projection * view;
        if !matrix.is_finite() || !matrix.determinant().is_finite() || matrix.determinant() == 0.0 {
            return Err(InvalidFit);
        }
        Ok(matrix)
    }
}
