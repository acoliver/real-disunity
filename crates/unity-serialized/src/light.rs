//! The `Light` layout.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// Reads a value followed by the flag that says whether Unity uses it, then padding.
/// Unity ignores the value when the flag is off, so it is dropped.
fn optional<'a, T>(
    r: &mut ObjectReader<'a>,
    read: impl FnOnce(&mut ObjectReader<'a>) -> Result<T, SerializedError>,
) -> Result<Option<T>, SerializedError> {
    let value = read(r)?;
    let used = r.bool()?;
    r.align()?;
    Ok(used.then_some(value))
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShadowSettings {
    /// Unity's `LightShadows`: 0 none, 1 hard, 2 soft.
    pub kind: i32,
    pub resolution: i32,
    pub custom_resolution: i32,
    pub strength: f32,
    pub bias: f32,
    pub normal_bias: f32,
    pub near_plane: f32,
    /// A row-major culling matrix that replaces the computed one.
    pub culling_matrix_override: Option<[f32; 16]>,
    pub use_view_frustum_for_caster_culling: bool,
}

impl ShadowSettings {
    /// Reads the settings serialized together. The view frustum flag is serialized near the
    /// end of the light, so this returns a constructor that takes it.
    fn read(
        r: &mut ObjectReader<'_>,
    ) -> Result<impl FnOnce(bool) -> Self + use<>, SerializedError> {
        let [kind, resolution, custom_resolution] = [r.i32()?, r.i32()?, r.i32()?];
        let [strength, bias, normal_bias, near_plane] = r.floats()?;
        let culling_matrix_override = optional(r, ObjectReader::floats)?;
        Ok(move |use_view_frustum_for_caster_culling| Self {
            kind,
            resolution,
            custom_resolution,
            strength,
            bias,
            normal_bias,
            near_plane,
            culling_matrix_override,
            use_view_frustum_for_caster_culling,
        })
    }
}

/// What lightmap baking produced for the light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightBakingOutput {
    pub probe_occlusion_light_index: i32,
    pub occlusion_mask_channel: i32,
    pub lightmap_bake_type: i32,
    pub mixed_lighting_mode: i32,
    pub is_baked: bool,
}

impl LightBakingOutput {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let output = Self {
            probe_occlusion_light_index: r.i32()?,
            occlusion_mask_channel: r.i32()?,
            lightmap_bake_type: r.i32()?,
            mixed_lighting_mode: r.i32()?,
            is_baked: r.bool()?,
        };
        r.align()?;
        Ok(output)
    }
}

/// Spot light cone angles, in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub angle: f32,
    pub inner_angle: f32,
    pub reflector: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub game_object: PPtr,
    pub enabled: bool,
    /// Unity's `LightType`: 0 spot, 1 directional, 2 point, 3 rectangle, 4 disc.
    pub kind: i32,
    /// Color as r, g, b, a.
    pub color: [f32; 4],
    pub intensity: f32,
    pub range: f32,
    pub spot: Spot,
    pub cookie_size: [f32; 2],
    pub shadows: ShadowSettings,
    pub cookie: PPtr,
    pub draw_halo: bool,
    pub baking: LightBakingOutput,
    pub flare: PPtr,
    pub render_mode: i32,
    pub culling_mask: u32,
    pub rendering_layer_mask: u32,
    pub lightmapping: i32,
    pub shadow_caster_mode: i32,
    pub area_size: [f32; 2],
    pub bounce_intensity: f32,
    /// Color temperature in kelvin, when the light uses one.
    pub color_temperature: Option<f32>,
    /// Bounding sphere as center and radius, when it replaces the computed one.
    pub bounding_sphere_override: Option<[f32; 4]>,
    pub force_visible: bool,
    pub light_unit: i32,
    pub lux_at_distance: f32,
}

/// The fields after the bounce intensity, in serialized order.
struct LightTail {
    color_temperature: Option<f32>,
    bounding_sphere_override: Option<[f32; 4]>,
    use_view_frustum_for_caster_culling: bool,
    force_visible: bool,
    light_unit: i32,
    lux_at_distance: f32,
    spot_reflector: bool,
}

impl LightTail {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let color_temperature = optional(r, ObjectReader::f32)?;
        let sphere = r.floats()?;
        let [use_sphere, use_view_frustum, force_visible] = [r.bool()?, r.bool()?, r.bool()?];
        r.align()?;
        let tail = Self {
            color_temperature,
            bounding_sphere_override: use_sphere.then_some(sphere),
            use_view_frustum_for_caster_culling: use_view_frustum,
            force_visible,
            light_unit: r.i32()?,
            lux_at_distance: r.f32()?,
            spot_reflector: r.bool()?,
        };
        r.align()?;
        Ok(tail)
    }
}

impl Layout for Light {
    const CLASS: ClassId = ClassId::LIGHT;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        let kind = r.i32()?;
        let color = r.floats()?;
        let [intensity, range, angle, inner_angle] = r.floats()?;
        let cookie_size = r.floats()?;
        let shadows = ShadowSettings::read(r)?;
        let cookie = r.pptr()?;
        let draw_halo = r.bool()?;
        r.align()?;
        let baking = LightBakingOutput::read(r)?;
        let flare = r.pptr()?;
        let render_mode = r.i32()?;
        let [culling_mask, rendering_layer_mask] = [r.u32()?, r.u32()?];
        let [lightmapping, shadow_caster_mode] = [r.i32()?, r.i32()?];
        let area_size = r.floats()?;
        let bounce_intensity = r.f32()?;
        let tail = LightTail::read(r)?;
        let shadows = shadows(tail.use_view_frustum_for_caster_culling);
        Ok(Self {
            game_object,
            enabled,
            kind,
            color,
            intensity,
            range,
            spot: Spot {
                angle,
                inner_angle,
                reflector: tail.spot_reflector,
            },
            cookie_size,
            shadows,
            cookie,
            draw_halo,
            baking,
            flare,
            render_mode,
            culling_mask,
            rendering_layer_mask,
            lightmapping,
            shadow_caster_mode,
            area_size,
            bounce_intensity,
            color_temperature: tail.color_temperature,
            bounding_sphere_override: tail.bounding_sphere_override,
            force_visible: tail.force_visible,
            light_unit: tail.light_unit,
            lux_at_distance: tail.lux_at_distance,
        })
    }
}
