//! The values the host supplies under the names Unity's shaders read for every draw and every
//! camera. render-program names none of them; these are the per-draw and camera layers.
//!
//! # Which names, and the evidence for each
//!
//! The rules for this repository forbid Unity's documentation and implementation, so a name is
//! here only when the player's own parameter tables show it and its words say what it holds.
//! `unity-import player-shader-builtins` counts, over the 10,773 Vulkan variants of the Old World
//! player, the constant buffer members that no shader declares as a material property. These
//! members are among them, as 4x4 float matrices unless noted, in the numbers of variants given
//! (a variant holds a name in the buffer of its vertex stage, of its fragment stage, or in
//! `UnityPerDraw`):
//!
//! | Name | Variants |
//! | --- | --- |
//! | `unity_ObjectToWorld` | 3,007 in `UnityPerDraw`, 414 vertex, 13 fragment |
//! | `unity_WorldToObject` | 2,728 in `UnityPerDraw`, 191 vertex |
//! | `unity_MatrixV` | 2,785 fragment, 147 vertex |
//! | `unity_MatrixVP` | 8,685 vertex, 39 fragment |
//! | `_WorldSpaceCameraPos` (three floats) | 3,562 vertex, 2,809 fragment |
//!
//! Each holds what its words say: the matrix from the object to the world, its inverse, the
//! world to camera matrix, the world to clip matrix and the camera's position in the world.
//! A name whose meaning the words do not say is left out, and so is each value whose
//! convention nothing in the repository measures; see the crate documentation for the list.
//!
//! # Values
//!
//! * `unity_ObjectToWorld` is the renderer's local-to-world matrix, column by column, as the
//!   world stores it. The layer copies sixteen floats in that order, and the world stores
//!   matrices a script sets for a material the same way, so a built-in matrix and a property
//!   matrix of one shader have one memory order.
//! * `unity_WorldToObject` is its inverse, left out when the matrix has none.
//! * `unity_MatrixV` is [`world_to_camera`].
//! * `unity_MatrixVP` is [`clip_from_gl`] times the OpenGL projection times the view.
//! * `_WorldSpaceCameraPos` is the translation of the inverse of the view.

use glam::{Mat4, Vec4};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;

use super::camera::{clip_from_gl, projection, world_to_camera};
use super::snapshot::CameraView;

pub const OBJECT_TO_WORLD: &str = "unity_ObjectToWorld";
pub const WORLD_TO_OBJECT: &str = "unity_WorldToObject";
pub const VIEW: &str = "unity_MatrixV";
pub const VIEW_PROJECTION: &str = "unity_MatrixVP";
pub const CAMERA_POSITION: &str = "_WorldSpaceCameraPos";

/// A matrix is singular when its determinant is this small next to 1.
const SINGULAR: f32 = 1.0e-12;

/// The per-draw values of a renderer at `local_to_world`.
#[must_use]
pub fn draw_layer(local_to_world: &[f32; 16]) -> PropertyLayer {
    let matrix = Mat4::from_cols_array(local_to_world);
    let mut layer = PropertyLayer::default();
    layer
        .values
        .set(OBJECT_TO_WORLD, PropertyValue::Matrix(*local_to_world));
    if matrix.determinant().abs() > SINGULAR {
        layer.values.set(
            WORLD_TO_OBJECT,
            PropertyValue::Matrix(matrix.inverse().to_cols_array()),
        );
    }
    layer
}

/// The values of a camera that draws into a target `aspect` wide over high.
#[must_use]
pub fn camera_layer(view: &CameraView, aspect: f32) -> PropertyLayer {
    let world_to_camera = world_to_camera(view);
    let view_projection = clip_from_gl() * projection(&view.settings, aspect) * world_to_camera;
    let position: Vec4 = world_to_camera.inverse().w_axis;
    let mut layer = PropertyLayer::default();
    layer
        .values
        .set(VIEW, PropertyValue::Matrix(world_to_camera.to_cols_array()));
    layer.values.set(
        VIEW_PROJECTION,
        PropertyValue::Matrix(view_projection.to_cols_array()),
    );
    layer.values.set(
        CAMERA_POSITION,
        PropertyValue::Vector([position.x, position.y, position.z, 0.0]),
    );
    layer
}
