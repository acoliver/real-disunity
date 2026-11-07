//! A camera's view and projection, the way the engine layer defines them, and the clip space
//! render-program draws in.
//!
//! The engine layer defines both matrices in OpenGL's conventions: camera space looks down
//! negative Z, and depth maps to -1..1. The view is a scale of Z by -1 after the inverse of the
//! camera object's position and rotation (its scale is ignored), a view a script set replaces it.
//! The projection is a perspective one from the vertical field of view and the aspect, or an
//! orthographic one from half the visible height and the aspect, and a projection a script set
//! replaces it. [`clip_from_gl`] then maps those clip coordinates onto what render-program's
//! conventions tests measured for a Unity Vulkan module.

use glam::{Mat3, Mat4, Quat, Vec3, Vec4};
use worldrunner_world_simulation::presentation::{Camera, Projection};

use worldrunner_world_simulation::graphics::CameraView;

/// The matrix that takes OpenGL clip coordinates to the clip space a Unity Vulkan module is
/// drawn in.
///
/// render-program's conventions tests (`tests/program/conventions.rs`) measure that the module's
/// position output is used unflipped, so positive clip Y lands in the bottom rows of the image,
/// and that the depth that survives clipping is 0 to w. The OpenGL projection's Y therefore
/// stays as it is, which puts the lowest part of the scene in row 0, the same orientation as a
/// texture the player stored (its rows are bottom first). Only depth changes: `z' = (z + w) / 2`
/// takes -w..w to 0..w. Depth is not reversed, because render-program resolves the pass's
/// depth test as the shader states it.
#[must_use]
pub fn clip_from_gl() -> Mat4 {
    Mat4::from_cols(
        Vec4::X,
        Vec4::Y,
        Vec4::new(0.0, 0.0, 0.5, 0.0),
        Vec4::new(0.0, 0.0, 0.5, 1.0),
    )
}

/// The camera object's position and rotation, without its scale.
fn pose(world_matrix: &Mat4) -> Mat4 {
    let axes = [
        world_matrix.x_axis,
        world_matrix.y_axis,
        world_matrix.z_axis,
    ]
    .map(|axis| axis.truncate().normalize_or_zero());
    let rotation = if axes.iter().all(|axis| *axis != Vec3::ZERO) {
        Quat::from_mat3(&Mat3::from_cols(axes[0], axes[1], axes[2])).normalize()
    } else {
        Quat::IDENTITY
    };
    Mat4::from_rotation_translation(rotation, world_matrix.w_axis.truncate())
}

/// The camera's view: the matrix a script set, or the derived one.
#[must_use]
pub fn world_to_camera(view: &CameraView) -> Mat4 {
    view.settings.world_to_camera.map_or_else(
        || {
            let pose = pose(&Mat4::from_cols_array(&view.world_matrix));
            Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)) * pose.inverse()
        },
        |matrix| Mat4::from_cols_array(&matrix),
    )
}

/// A perspective projection for a vertical field of view in degrees.
fn perspective(fov_degrees: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_degrees.to_radians() * 0.5).tan();
    Mat4::from_cols(
        Vec4::new(f / aspect, 0.0, 0.0, 0.0),
        Vec4::new(0.0, f, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -(far + near) / (far - near), -1.0),
        Vec4::new(0.0, 0.0, -2.0 * far * near / (far - near), 0.0),
    )
}

/// An orthographic projection of a box `half_width` and `half_height` around the view axis.
fn orthographic(half_width: f32, half_height: f32, near: f32, far: f32) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(1.0 / half_width, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0 / half_height, 0.0, 0.0),
        Vec4::new(0.0, 0.0, -2.0 / (far - near), 0.0),
        Vec4::new(0.0, 0.0, -(far + near) / (far - near), 1.0),
    )
}

/// The camera's OpenGL projection for a target `aspect` wide over high: the matrix a script set,
/// or the derived one.
#[must_use]
pub fn projection(settings: &Camera, aspect: f32) -> Mat4 {
    if let Some(matrix) = settings.projection_matrix {
        return Mat4::from_cols_array(&matrix);
    }
    match settings.projection {
        Projection::Perspective => {
            perspective(settings.field_of_view, aspect, settings.near, settings.far)
        }
        Projection::Orthographic => orthographic(
            settings.half_height * aspect,
            settings.half_height,
            settings.near,
            settings.far,
        ),
    }
}
