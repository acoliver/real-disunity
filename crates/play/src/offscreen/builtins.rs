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
//! * A blit with a material draws a quad over clip space with no renderer and no camera
//!   ([`blit_layer`]): `unity_ObjectToWorld`, `unity_WorldToObject` and `unity_MatrixVP` are the
//!   identity. Old World's terrain bake blits its normal through a pass of the post-processing
//!   material whose vertex stage reads `unity_ObjectToWorld` and `unity_MatrixVP` and nothing else
//!   per draw (the constants of the pass as the engine packs them), so with no value for them the
//!   quad collapsed to a point and the tile normal stayed empty.
//! * `unity_MatrixV` is [`world_to_camera`](super::camera::world_to_camera).
//! * `unity_MatrixVP` is [`clip_from_gl`](super::camera::clip_from_gl) times the OpenGL
//!   projection times the view. The projection is the engine layer's; the clip space is the one
//!   `render-program`'s conventions tests measured, with depth reversed: near is w and far is 0
//!   (see the module documentation of [`offscreen`](super) for the evidence). The projection
//!   builtins `glstate_matrix_projection` and `_ZBufferParams` are not supplied (see "Left
//!   missing"), so a shader that reads them gets zero.
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
//!   They are the fallback in the frame's starting globals: a value the world's globals hold, or
//!   that a command buffer sets later with `SetGlobalVector`, replaces them from that place on.
//!   The light's colour temperature is not applied: nothing here says how the player converts it.
//! * `unity_SHAr`, `unity_SHAg`, `unity_SHAb`, `unity_SHBr`, `unity_SHBg`, `unity_SHBb`, `unity_SHC`
//!   and `unity_SpecCube0_HDR` are the default ambient of [`add_ambient`], in the frame's starting
//!   globals like the main light (see "Left missing" for what it is and is not).
//! * `unity_OrthoParams` (2,766 variants) carries the projection kind in its fourth component: 1
//!   for a view whose projection has no perspective divide and 0 for one that has it
//!   ([`view_layer`]). In the same shaders (`Mohawk/StandardShader`, `LakeShader`) the fragment
//!   stage compares that component with 0 and nothing else: 0 takes the view vector from
//!   `_WorldSpaceCameraPos` to the surface, anything else takes the third row of `unity_MatrixV`.
//!   The first two components are the half width and half height of the view. The captured frames
//!   of the original's perspective camera hold (8.889, 5, 0, 0) at 1280 by 720: the camera's
//!   orthographic half height of 5, kept while it draws in perspective, and that times the aspect
//!   of 16 to 9. The host writes the same for a camera, from the camera's half height and the
//!   aspect of the projection, and for an orthographic projection the two sizes come from its x
//!   and y scales. No scanned stage reads the first three components.
//! * `_ProjectionParams` (3,522) holds +1, the near-plane distance, the far-plane distance and
//!   its reciprocal ([`view_layer`]). The captured frames of the original hold
//!   (1, 0.3, 209.9977, 0.004762) for its main camera and (1, 0.3, 1000, 0.001) for another. The
//!   host derives both distances from the z row of the projection the view draws with, so a view
//!   a script gave an explicit projection holds them too, and the far plane carries the rounding
//!   of the matrix (a far plane of 210 gives 209.9977 in both). `LakeShader`'s vertex stage puts
//!   `(0.5 (x + w), 0.5 (y * _ProjectionParams.x + w))` in its screen position and the fragment
//!   samples the depth texture there. Play draws positive clip y into the high rows (see
//!   [`clip_from_gl`](super::camera::clip_from_gl)), so the fragment addresses its own texel
//!   only with +1.
//!   `Mohawk/Transparent/WaterShader` ends its view-space reflection ray at
//!   `z = -_ProjectionParams.y` when it would pass the near plane, so the second component is
//!   the near distance. A projection with no finite near or far plane leaves the name missing.
//! * `unity_WorldTransformParams` (2,011) carries the sign of the object-to-world determinant in
//!   its fourth component, -1 for a mirrored renderer and 1 otherwise, including a singular one
//!   ([`draw_layer`]). `Mohawk/StandardShader`'s vertex stage multiplies its bitangent,
//!   `cross(normal, tangent) * tangent.w`, by `w >= 0 ? 1 : -1`. For a rotation, reflection or
//!   uniform scale `M`, `cross(M^-T n, M t)` is `sign(det M)` times the rotated `n x t`, so that
//!   factor restores the bitangent's direction only when it is the determinant's sign. No
//!   scanned stage reads the first three components, which are 0.
//! * `unity_LightData` (1,472 variants) carries the main light's factor in its third component:
//!   `(0, 0, z, 0)` with `z` 1 when the main light affects the draw's renderer and 0 when it does
//!   not (the `light_data` module has the rule). A read-only decode of the player's own
//!   `Mohawk/Terrain/TerrainShader` and `Mohawk/StandardShader` shows
//!   `mainLight = _MainLightColor.rgb * (shadowAtten * unity_LightData.z *
//!   saturate(dot(N, _MainLightPosition.xyz)))` and an output of `brdf * mainLight + ambient`,
//!   and all 116 stage modules of those shaders that read `unity_LightData` read only `.z`. The other three components are 0. The value is a per-draw
//!   one, so it wins over a `--global unity_LightData=` value and over a script's global for a
//!   frame that has a directional light; a frame with none leaves the name to those.
//! * `_ScreenParams` (2,389) holds the width and height in pixels of the target the view draws
//!   into, the mip's for a mip target, then 1 + 1 / width and 1 + 1 / height ([`view_layer`]); the
//!   original's frames hold (1280, 720, 1.00078, 1.00139). `Mohawk/StandardShader` multiplies the
//!   0..1 screen position by its first two components and then by 0.25 for a four-pixel tile. A
//!   command-list draw with no bound target has none.
//!
//! # Left missing
//!
//! `render-program` lists a member no layer supplies in `OffscreenOperations::missing` and
//! leaves it zero. These are left to it, with the count of the player's 10,773 Vulkan variants
//! that hold each (a variant holds a name once per buffer):
//!
//! * `_ZBufferParams` (616) and `_Time` (2,688): the names do not say what their four floats
//!   hold, and the game's own arithmetic does not settle it. `_Time` is only multiplied by
//!   authored speeds, so no rate follows; `_ZBufferParams` turns two depths into
//!   `1 / (z * depth + w)`, subtracts them and divides by authored values, which fixes `z / w`
//!   for the depth range but not the scale.
//! * `glstate_matrix_projection` (109), `unity_MatrixInvV` (5), `unity_MatrixInvVP` (4),
//!   `unity_CameraProjection` (4), `unity_CameraInvProjection` (2), `unity_WorldToCamera` (4)
//!   and `unity_CameraToWorld` (2): the words name the matrix, and each is too rare to be
//!   evidence that the engine sets it in the way the common ones are set. The projection ones
//!   would also need a choice between the engine layer's projection and the one adjusted for the
//!   graphics API, which nothing here measures.
//! * The other lighting and shadow values (the `LightShadows` buffer): they depend on shadow maps
//!   and additional lights that no offscreen record carries.
//!   `unity_SHAr` and the other spherical harmonics (1,040 variants) and `unity_SpecCube0_HDR`
//!   (1,552) are not left to render-program either: [`add_ambient`] gives them an engine
//!   default, because a shader that reads them as zero draws black. The default is flat (0.3 in
//!   the fourth component of `unity_SHAr`, `unity_SHAg` and `unity_SHAb`, zero in the rest) and
//!   (1, 1, 0, 0) for `unity_SpecCube0_HDR`, a neutral decode chosen without a measured
//!   convention. It stands for a world whose scene wrote no environment.
//! * Instancing (`unity_ObjectToWorldArray` and `unity_WorldToObjectArray` in 2,062 variants,
//!   `unity_BaseInstanceID` 2,122, and the DOTS tables): which instance index a draw reads is
//!   not decided, and each draw here is one sub-mesh.
//! * `<texture>_ST` (7,265 of the 38,160 texture bindings have a four-float member of that
//!   name): the world carries a texture slot's scale and offset, and the member that holds them
//!   has the texture's name plus `_ST`, but the order of its four floats is in nothing the
//!   repository defines. `<texture>_TexelSize` (1,980) is not left to the host:
//!   `render-program` fills it with the size of the texture bound under `<texture>`.

use glam::{Mat4, Vec4};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;

use super::camera::{clip_from_gl, projection, world_to_camera};
use crate::scene_light::{AMBIENT_SKY_IRRADIANCE, main_light_index};
use worldrunner_world_simulation::graphics::{CameraView, WorldLight};

pub const OBJECT_TO_WORLD: &str = "unity_ObjectToWorld";
pub const WORLD_TO_OBJECT: &str = "unity_WorldToObject";
pub const VIEW: &str = "unity_MatrixV";
pub const VIEW_PROJECTION: &str = "unity_MatrixVP";
pub const CAMERA_POSITION: &str = "_WorldSpaceCameraPos";
pub const MAIN_LIGHT_POSITION: &str = "_MainLightPosition";
pub const MAIN_LIGHT_COLOR: &str = "_MainLightColor";
pub const ORTHO_PARAMS: &str = "unity_OrthoParams";
pub const PROJECTION_PARAMS: &str = "_ProjectionParams";
pub const SCREEN_PARAMS: &str = "_ScreenParams";
pub const WORLD_TRANSFORM_PARAMS: &str = "unity_WorldTransformParams";
pub const SH_AR: &str = "unity_SHAr";
pub const SH_AG: &str = "unity_SHAg";
pub const SH_AB: &str = "unity_SHAb";
pub const SH_BR: &str = "unity_SHBr";
pub const SH_BG: &str = "unity_SHBg";
pub const SH_BB: &str = "unity_SHBb";
pub const SH_C: &str = "unity_SHC";
pub const SPEC_CUBE0_HDR: &str = "unity_SpecCube0_HDR";

/// A matrix is singular when its determinant is this small next to 1.
const SINGULAR: f32 = 1.0e-12;

/// The per-draw values of a draw whose object matrices the shader reads per instance from a
/// buffer: the sign of the matrix determinant is not known, so the handedness is positive.
#[must_use]
pub fn instanced_draw_layer() -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    layer.values.set(
        WORLD_TRANSFORM_PARAMS,
        PropertyValue::Vector([0.0, 0.0, 0.0, 1.0]),
    );
    layer
}

/// The per-draw values of a renderer at `local_to_world`.
#[must_use]
pub fn draw_layer(local_to_world: &[f32; 16]) -> PropertyLayer {
    let matrix = Mat4::from_cols_array(local_to_world);
    let mut layer = instanced_draw_layer();
    layer
        .values
        .set(OBJECT_TO_WORLD, PropertyValue::Matrix(*local_to_world));
    if matrix.determinant() < 0.0 {
        layer.values.set(
            WORLD_TRANSFORM_PARAMS,
            PropertyValue::Vector([0.0, 0.0, 0.0, -1.0]),
        );
    }
    if matrix.determinant().abs() > SINGULAR {
        layer.values.set(
            WORLD_TO_OBJECT,
            PropertyValue::Matrix(matrix.inverse().to_cols_array()),
        );
    }
    layer
}

/// The values a blit with a material draws with. The blit's quad is in clip space (see
/// `adapter::blit_mesh`), so the pass places it with the identity as its object-to-world,
/// world-to-object and world-to-clip matrices. A blit has no renderer and no camera, so no other
/// layer holds them, and without them a vertex stage that multiplies its position by one packs a
/// zero matrix and puts every vertex at the origin.
#[must_use]
pub fn blit_layer() -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    for name in [OBJECT_TO_WORLD, WORLD_TO_OBJECT, VIEW_PROJECTION] {
        layer
            .values
            .set(name, PropertyValue::Matrix(Mat4::IDENTITY.to_cols_array()));
    }
    layer
}

/// The values of a camera that draws into a target of `size` pixels.
#[must_use]
pub fn camera_layer(view: &CameraView, size: [u32; 2]) -> PropertyLayer {
    let aspect = pixels(size[0]) / pixels(size[1]);
    view_layer(
        &world_to_camera(view),
        &projection(&view.settings, aspect),
        Some(size),
        Some(view.settings.half_height),
    )
}

/// The values of a view from the world to the camera `world_to_camera` and the OpenGL projection
/// `projection`, which a script sets as they are, drawn into a target of `size` pixels when one is
/// bound. `camera_half_height` is the orthographic half height of the camera the view belongs to,
/// which `unity_OrthoParams` keeps while the camera draws in perspective.
#[must_use]
pub fn view_layer(
    world_to_camera: &Mat4,
    projection: &Mat4,
    size: Option<[u32; 2]>,
    camera_half_height: Option<f32>,
) -> PropertyLayer {
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
        PropertyValue::Vector(ortho_params(projection, camera_half_height)),
    );
    if let Some((near, far)) = clip_planes(projection) {
        layer.values.set(
            PROJECTION_PARAMS,
            PropertyValue::Vector([1.0, near, far, far.recip()]),
        );
    }
    if let Some([width, height]) = size.map(|size| size.map(pixels)) {
        layer.values.set(
            SCREEN_PARAMS,
            PropertyValue::Vector([width, height, 1.0 + width.recip(), 1.0 + height.recip()]),
        );
    }
    layer
}

/// A light's forward axis shorter than this points in no direction.
const SHORTEST_AXIS: f32 = 1.0e-6;

/// Adds the main light of `lights` to a layer as its fallback: `_MainLightPosition` is the unit
/// direction from a surface toward the light and `_MainLightColor` the light's colour times its
/// intensity, each with 0 in the fourth component. The main light is the one the retained scene
/// follows ([`main_light_index`]). With no directional light, or one whose forward axis has no
/// length, both stay missing. A name the layer already holds keeps its value.
///
/// A frame adds its light to the globals the script starts from, below every global the world or
/// the script sets, so a light the game's own pipeline sets as a global replaces the host's at
/// the place in the frame where it sets it (see `Mapper::new`).
/// A default a shader declares for either name sits below the globals too (see
/// `split_below_globals`), so the host's light is not hidden by it.
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
    if layer.values.get(MAIN_LIGHT_POSITION).is_none() {
        layer.values.set(
            MAIN_LIGHT_POSITION,
            PropertyValue::Vector([toward.x, toward.y, toward.z, 0.0]),
        );
    }
    if layer.values.get(MAIN_LIGHT_COLOR).is_none() {
        layer.values.set(
            MAIN_LIGHT_COLOR,
            PropertyValue::Vector([r * intensity, g * intensity, b * intensity, 0.0]),
        );
    }
}

/// The neutral decode of `unity_SpecCube0_HDR` the default ambient gives: no repository evidence
/// measures how the player decodes its reflection probe, so the value is chosen as 1 for the two
/// scale terms and 0 for the others.
const NEUTRAL_SPEC_CUBE_HDR: [f32; 4] = [1.0, 1.0, 0.0, 0.0];

/// Adds the engine's default ambient lighting to a layer, for each name the layer holds no value
/// for: a flat ambient light of [`AMBIENT_SKY_IRRADIANCE`] in the fourth component of
/// `unity_SHAr`, `unity_SHAg` and `unity_SHAb`, zero in `unity_SHBr`, `unity_SHBg`, `unity_SHBb`
/// and `unity_SHC`, and (1, 1, 0, 0) in `unity_SpecCube0_HDR`.
///
/// This is an engine default for a frame in which nothing sets these names, and nothing more.
/// A world whose scene wrote an environment gets its ambient light from `add_environment`, which
/// packs the scene's probe; this flat ambient stands for a world that wrote none.
/// (1, 1, 0, 0) is a neutral decode chosen without a measured convention.
/// Without any value these names pack as zero and the scene shaders of the Old World player draw
/// black.
///
/// A frame adds the default to the globals its script starts from, next to the main light (see
/// `Mapper::new`), so a global the world holds and a `SetGlobalVector` of the script replace it.
pub fn add_ambient(layer: &mut PropertyLayer) {
    let flat = [0.0, 0.0, 0.0, AMBIENT_SKY_IRRADIANCE];
    let defaults = [
        (SH_AR, flat),
        (SH_AG, flat),
        (SH_AB, flat),
        (SH_BR, [0.0; 4]),
        (SH_BG, [0.0; 4]),
        (SH_BB, [0.0; 4]),
        (SH_C, [0.0; 4]),
        (SPEC_CUBE0_HDR, NEUTRAL_SPEC_CUBE_HDR),
    ];
    for (name, value) in defaults {
        if layer.values.get(name).is_none() {
            layer.values.set(name, PropertyValue::Vector(value));
        }
    }
}

/// A side in pixels as a float. Sides above 65535 saturate, and no render texture reaches them.
fn pixels(side: u32) -> f32 {
    f32::from(u16::try_from(side).unwrap_or(u16::MAX))
}

/// The near and far plane distances of an OpenGL `projection`, from the z row: with `s` the row's
/// z scale (`z_axis.z`) and `t` its translation (`w_axis.z`), a perspective projection has
/// `n = t / (s - 1)` and `f = t / (s + 1)`, and an orthographic one `n = (t + 1) / s` and
/// `f = (t - 1) / s`. `None` when the matrix has no finite near plane or no finite far plane
/// other than 0. The far plane is a difference of two nearly equal numbers for a distant one, so
/// it carries the rounding of the matrix: a camera with far 210 gives 209.9977 here and in the
/// captured frames of the original. A script projection with an oblique near plane rewrites that
/// row, so for it the values are no longer the camera's distances.
fn clip_planes(projection: &Mat4) -> Option<(f32, f32)> {
    let (scale, offset) = (projection.z_axis.z, projection.w_axis.z);
    let (near, far) = if orthographic_flag(projection) > 0.0 {
        ((offset + 1.0) / scale, (offset - 1.0) / scale)
    } else {
        (offset / (scale - 1.0), offset / (scale + 1.0))
    };
    (near.is_finite() && far.is_finite() && far.recip().is_finite()).then_some((near, far))
}

/// `unity_OrthoParams`: the half width and half height of the view, then 0 and the projection
/// kind. An orthographic projection holds both sizes in its x and y scales. A perspective one
/// holds none, and the player's frames carry the orthographic half height of the camera instead
/// (5 by default) with the half width that the aspect of the projection gives. A view of no
/// camera has 0 for them. The aspect is the ratio of the y scale to the x scale.
fn ortho_params(projection: &Mat4, camera_half_height: Option<f32>) -> [f32; 4] {
    let kind = orthographic_flag(projection);
    let (x_scale, y_scale) = (projection.x_axis.x, projection.y_axis.y);
    let half_height = if kind > 0.0 {
        y_scale.recip()
    } else {
        camera_half_height.unwrap_or(0.0)
    };
    let half_width = half_height * y_scale / x_scale;
    let finite = |value: f32| if value.is_finite() { value } else { 0.0 };
    [finite(half_width), finite(half_height), 0.0, kind]
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
