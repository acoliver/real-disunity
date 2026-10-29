//! Cameras and 3D lights of a player scene as plan components.
//!
//! Colors keep the numbers the player stored, as the project planner keeps its YAML colors.

use worldrunner_unity_serialized::{camera::Camera as Serialized, light::Light as SerializedLight};

use crate::{
    components::{Camera, CameraOutput, ClearFlags, Light3D},
    plan::PlanComponent,
};

/// The projection matrix mode the player's cameras serialize. The planner builds the lens from the
/// field of view or orthographic size, so any other mode is listed as not applied.
const STANDARD_PROJECTION_MODE: i32 = 1;
/// The color temperature the managed `Light` starts with.
const DEFAULT_COLOR_TEMPERATURE: f64 = 6_570.0;

/// The plan camera for a decoded one. Settings the managed camera cannot build are dropped
/// and added to `differences`, each naming `owner`.
///
/// # Errors
/// Fails when the clear flags are not one of the four serialized modes.
pub(crate) fn plan_camera(
    owner: &str,
    camera: &Serialized,
    differences: &mut Vec<String>,
) -> Result<PlanComponent, String> {
    let clear = match camera.clear_flags {
        1 => ClearFlags::Skybox,
        2 => ClearFlags::SolidColor,
        3 => ClearFlags::Depth,
        4 => ClearFlags::Nothing,
        other => return Err(format!("{owner}: camera clear flags {other} are not known")),
    };
    if !camera.output.target_texture.is_null() {
        differences.push(format!("{owner}: camera target texture is not built"));
    }
    if camera.projection_matrix_mode != STANDARD_PROJECTION_MODE {
        differences.push(format!(
            "{owner}: camera projection matrix mode {} is not applied",
            camera.projection_matrix_mode
        ));
    }
    Ok(PlanComponent::Camera(Camera {
        enabled: camera.enabled,
        clear,
        background: camera.background_color.map(f64::from),
        orthographic_size: camera.projection.orthographic_size.into(),
        near: camera.projection.near.into(),
        far: camera.projection.far.into(),
        depth: camera.depth.into(),
        orthographic: camera.projection.orthographic,
        field_of_view: camera.projection.field_of_view.into(),
        culling_mask: camera.culling_mask,
        rect: camera.viewport.map(f64::from),
        target_display: camera.output.target_display.into(),
        output: CameraOutput {
            allow_hdr: camera.output.hdr,
            allow_msaa: camera.output.allow_msaa,
            allow_dynamic_resolution: camera.output.allow_dynamic_resolution,
        },
        use_occlusion_culling: camera.occlusion_culling,
    }))
}

/// The plan light for a decoded one. A cookie or flare is dropped and added to `differences`.
///
/// # Errors
/// Fails when the light type or shadow mode is not a serialized one.
pub(crate) fn plan_light(
    owner: &str,
    light: &SerializedLight,
    differences: &mut Vec<String>,
) -> Result<PlanComponent, String> {
    let kind = match light.kind {
        0 => "Spot",
        1 => "Directional",
        2 => "Point",
        3 => "Rectangle",
        4 => "Disc",
        other => return Err(format!("{owner}: light type {other} is not known")),
    };
    let shadows = match light.shadows.kind {
        0 => "None",
        1 => "Hard",
        2 => "Soft",
        other => return Err(format!("{owner}: light shadow mode {other} is not known")),
    };
    if !light.cookie.is_null() {
        differences.push(format!("{owner}: light cookie is not built"));
    }
    if !light.flare.is_null() {
        differences.push(format!("{owner}: light flare is not built"));
    }
    let [cookie_width, cookie_height] = light.cookie_size;
    if cookie_width.to_bits() != cookie_height.to_bits() {
        differences.push(format!(
            "{owner}: light cookie size is not square; the width is used"
        ));
    }
    Ok(PlanComponent::Light {
        enabled: light.enabled,
        light: Light3D {
            kind: kind.to_owned(),
            color: light.color.map(f64::from),
            intensity: light.intensity.into(),
            range: light.range.into(),
            spot_angle: light.spot.angle.into(),
            inner_spot_angle: light.spot.inner_angle.into(),
            shadows: shadows.to_owned(),
            shadow_strength: light.shadows.strength.into(),
            shadow_bias: light.shadows.bias.into(),
            shadow_normal_bias: light.shadows.normal_bias.into(),
            shadow_near_plane: light.shadows.near_plane.into(),
            culling_mask: light.culling_mask,
            rendering_layer_mask: light.rendering_layer_mask,
            bounce_intensity: light.bounce_intensity.into(),
            color_temperature: light
                .color_temperature
                .map_or(DEFAULT_COLOR_TEMPERATURE, f64::from),
            use_color_temperature: light.color_temperature.is_some(),
            cookie_size: cookie_width.into(),
            lightmap_bake_type: light.lightmapping.into(),
        },
    })
}
