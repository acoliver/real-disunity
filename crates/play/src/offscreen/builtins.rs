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
//!
//! # Values
//!
//! * `unity_ObjectToWorld` is the renderer's local-to-world matrix, column by column, as the
//!   world stores it. A matrix a script sets on a material reaches a constant buffer in the
//!   same order (`render-program` copies the sixteen floats as four rows of 16 bytes), and every
//!   4x4 member of the player's tables has the same shape, so a built-in is written the way a
//!   script's matrix is. Which order a shader reads is not measured here; it is inferred from
//!   there being one way to write a matrix.
//! * `unity_WorldToObject` is its inverse, left out when the matrix has none.
//! * `unity_MatrixV` is [`world_to_camera`](super::camera::world_to_camera).
//! * `unity_MatrixVP` is [`clip_from_gl`](super::camera::clip_from_gl) times the OpenGL
//!   projection times the view. The projection is the engine layer's; the clip space is the one
//!   `render-program`'s conventions tests measured. Whether the player expects a reversed depth
//!   range is not known, so depth is not reversed.
//! * `_WorldSpaceCameraPos` is the translation of the inverse of the view, with 0 in the fourth
//!   component for a member of three floats to ignore.
//! * `_MainLightPosition` and `_MainLightColor` (four floats each) come from the main light of the
//!   snapshot's lights, the first directional one, the same light the retained scene follows
//!   ([`add_main_light`]). The player's tables show the names in 3,340 and 2,604 variants; their
//!   words do not say which way the direction points or whether the colour includes intensity, so
//!   that was read from the game's own shaders. Over every Vulkan variant of the player, the game's
//!   `Mohawk/StandardShader` (1,488 of 5,760 variants), `Mohawk/StandardShaderTransparent`
//!   (288 of 384), `Mohawk/Terrain/TerrainShader` (192 of 768) and `Custom/Lava` (192 of 256) hold
//!   both. In nine of their variants translated with naga, every fragment stage takes
//!   `clamp(dot(N, position.xyz), 0, 1)` and adds `position.xyz` to the view direction for the half
//!   vector, so the direction is a unit vector from the surface toward the light; it multiplies
//!   the diffuse and specular terms by `color.xyz` with no other intensity, so the colour holds the
//!   light's colour times its intensity. No stage reads either fourth component, which is 0 here.
//!   The light's colour temperature is not applied: nothing here says how the player converts it.
//! * `unity_OrthoParams` (2,766 variants) carries 1 in its fourth component for a view whose
//!   projection has no perspective divide and 0 for one that has it ([`view_layer`]). In the same
//!   shaders (`Mohawk/StandardShader`, `LakeShader`) the fragment stage compares that component
//!   with 0 and nothing else: 0 takes the view vector from `_WorldSpaceCameraPos` to the surface,
//!   anything else takes the third row of `unity_MatrixV`. The first three components are 0; no
//!   scanned stage reads them.
//! * `_ScreenParams` (2,389) holds the width and height in pixels of the target the view draws
//!   into, the mip's for a mip target ([`add_screen_size`]). `Mohawk/StandardShader` multiplies the
//!   0..1 screen position by its first two components and then by 0.25 for a four-pixel tile. The
//!   last two are 0; no scanned stage reads them. A command-list draw with no bound target has
//!   none.
//!
//! # Left missing
//!
//! `render-program` lists a member no layer supplies in `OffscreenOperations::missing` and
//! leaves it zero. These are left to it, with the count of the player's 10,773 Vulkan variants
//! that hold each (a variant holds a name once per buffer):
//!
//! * `_ProjectionParams` (3,522), `_ZBufferParams` (616), `_Time` (2,688) and
//!   `unity_WorldTransformParams` (2,011): the names do not say what their four floats hold, and
//!   the game's own arithmetic does not settle it. `_Time` is only multiplied by authored speeds,
//!   so no rate follows; `_ZBufferParams` turns two depths into `1 / (z * depth + w)`, subtracts
//!   them and divides by authored values, which fixes `z / w` for the depth range but not the
//!   scale; `_ProjectionParams.x` turns the screen y over in the vertex stage, and which way
//!   depends on the target's orientation, which is not established.
//! * `glstate_matrix_projection` (109), `unity_MatrixInvV` (5), `unity_MatrixInvVP` (4),
//!   `unity_CameraProjection` (4), `unity_CameraInvProjection` (2), `unity_WorldToCamera` (4)
//!   and `unity_CameraToWorld` (2): the words name the matrix, and each is too rare to be
//!   evidence that the engine sets it in the way the common ones are set. The projection ones
//!   would also need a choice between the engine layer's projection and the one adjusted for the
//!   graphics API, which nothing here measures.
//! * The other lighting and shadow values (the `LightShadows` buffer, `unity_SHAr` and the other
//!   spherical harmonics 1,040, `unity_SpecCube0_HDR` 1,552, `unity_LightData` 1,472): they
//!   depend on shadow maps, probes and additional lights that no offscreen record carries.
//! * Instancing (`unity_ObjectToWorldArray` and `unity_WorldToObjectArray` in 2,062 variants,
//!   `unity_BaseInstanceID` 2,122, and the DOTS tables): which instance index a draw reads is
//!   not decided, and each draw here is one sub-mesh.
//! * `<texture>_ST` (7,265 of the 38,160 texture bindings have a four-float member of that
//!   name) and `<texture>_TexelSize` (1,980): the world carries a texture slot's scale and
//!   offset, and the member that holds them has the texture's name plus `_ST`, but the order of
//!   its four floats is in nothing the repository defines. `render-program` supplies
//!   `_TexelSize` for a blit's source only.

use glam::{Mat4, Vec4};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;

use super::camera::{clip_from_gl, projection, world_to_camera};
use crate::scene_light::main_light_index;
use worldrunner_world_simulation::graphics::{CameraView, WorldLight};

pub const OBJECT_TO_WORLD: &str = "unity_ObjectToWorld";
pub const WORLD_TO_OBJECT: &str = "unity_WorldToObject";
pub const VIEW: &str = "unity_MatrixV";
pub const VIEW_PROJECTION: &str = "unity_MatrixVP";
pub const CAMERA_POSITION: &str = "_WorldSpaceCameraPos";
pub const MAIN_LIGHT_POSITION: &str = "_MainLightPosition";
pub const MAIN_LIGHT_COLOR: &str = "_MainLightColor";
pub const ORTHO_PARAMS: &str = "unity_OrthoParams";
pub const SCREEN_PARAMS: &str = "_ScreenParams";

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
    view_layer(&world_to_camera(view), &projection(&view.settings, aspect))
}

/// The values of a view from the world to the camera `world_to_camera` and the OpenGL projection
/// `projection`, which a script sets as they are.
#[must_use]
pub fn view_layer(world_to_camera: &Mat4, projection: &Mat4) -> PropertyLayer {
    let view_projection = clip_from_gl() * *projection * *world_to_camera;
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
    layer.values.set(
        ORTHO_PARAMS,
        PropertyValue::Vector([0.0, 0.0, 0.0, orthographic_flag(projection)]),
    );
    layer
}

/// A light's forward axis shorter than this points in no direction.
const SHORTEST_AXIS: f32 = 1.0e-6;

/// Adds the main light of `lights` to a camera's values: `_MainLightPosition` is the unit
/// direction from a surface toward the light and `_MainLightColor` the light's colour times its
/// intensity, each with 0 in the fourth component. The main light is the one the retained scene
/// follows ([`main_light_index`]). With no directional light, or one whose forward axis has no
/// length, both stay missing.
pub fn add_main_light(layer: &mut PropertyLayer, lights: &[WorldLight]) {
    let Some(index) = main_light_index(lights.iter().map(|light| &light.settings)) else {
        return;
    };
    let light = &lights[index];
    let forward = Mat4::from_cols_array(&light.world_matrix).z_axis.truncate();
    let length = forward.length();
    if !length.is_finite() || length < SHORTEST_AXIS {
        return;
    }
    let toward = -forward / length;
    let [r, g, b, _] = light.settings.color;
    let intensity = light.settings.intensity;
    layer.values.set(
        MAIN_LIGHT_POSITION,
        PropertyValue::Vector([toward.x, toward.y, toward.z, 0.0]),
    );
    layer.values.set(
        MAIN_LIGHT_COLOR,
        PropertyValue::Vector([r * intensity, g * intensity, b * intensity, 0.0]),
    );
}

/// Adds the size in pixels of the target a view draws into: `_ScreenParams` holds the width and
/// height, with 0 in the last two components, which no scanned shader reads.
pub fn add_screen_size(layer: &mut PropertyLayer, size: [u32; 2]) {
    let [width, height] = size.map(pixels);
    layer.values.set(
        SCREEN_PARAMS,
        PropertyValue::Vector([width, height, 0.0, 0.0]),
    );
}

/// A side in pixels as a float. Sides above 65535 saturate, and no render texture reaches them.
fn pixels(side: u32) -> f32 {
    f32::from(u16::try_from(side).unwrap_or(u16::MAX))
}

/// 1 for a projection without a perspective divide, whose clip w does not depend on depth, and 0
/// for one with it.
fn orthographic_flag(projection: &Mat4) -> f32 {
    if projection.row(3).z.abs() < f32::MIN_POSITIVE {
        1.0
    } else {
        0.0
    }
}
