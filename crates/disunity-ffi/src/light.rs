//! 3D light component settings. The rules live in world-simulation; this file converts records.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError, physics::component};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle,
    presentation::{Light, LightBakeType, LightKind, LightSettings, LightShadowMode},
};

/// A light's settings. `kind` uses the managed `LightType` values (0 Spot, 1 Directional,
/// 2 Point, 3 Rectangle, 4 Disc, 5 Pyramid, 6 Box, 7 Tube), `shadows` is 0 None, 1 Hard or 2 Soft,
/// and `lightmap_bake_type` is 1 Mixed, 2 Baked or 4 Realtime. The masks have one bit per layer,
/// `cookie_texture` is a managed texture's instance id (0 for none) and booleans are 0 or 1.
/// A write brings the spot angles and shadow strength into range, as world-simulation defines.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityLight {
    pub cookie_texture: u64,
    pub enabled: u32,
    pub kind: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub intensity: f32,
    pub range: f32,
    pub spot_angle: f32,
    pub inner_spot_angle: f32,
    pub shadows: u32,
    pub shadow_strength: f32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_near_plane: f32,
    pub culling_mask: u32,
    pub rendering_layer_mask: u32,
    pub bounce_intensity: f32,
    pub color_temperature: f32,
    pub use_color_temperature: u32,
    pub cookie_size: f32,
    pub lightmap_bake_type: u32,
}

impl Default for DisunityLight {
    /// The settings of a new light component.
    fn default() -> Self {
        Self::from_native(Light::default())
    }
}

impl DisunityLight {
    fn from_native(light: Light) -> Self {
        let s = light.settings;
        let [r, g, b, a] = s.color;
        Self {
            cookie_texture: s.cookie_texture,
            enabled: u32::from(light.enabled),
            kind: s.kind as u32,
            r,
            g,
            b,
            a,
            intensity: s.intensity,
            range: s.range,
            spot_angle: s.spot_angle,
            inner_spot_angle: s.inner_spot_angle,
            shadows: s.shadows as u32,
            shadow_strength: s.shadow_strength,
            shadow_bias: s.shadow_bias,
            shadow_normal_bias: s.shadow_normal_bias,
            shadow_near_plane: s.shadow_near_plane,
            culling_mask: s.culling_mask,
            rendering_layer_mask: s.rendering_layer_mask,
            bounce_intensity: s.bounce_intensity,
            color_temperature: s.color_temperature,
            use_color_temperature: u32::from(s.use_color_temperature),
            cookie_size: s.cookie_size,
            lightmap_bake_type: s.bake_type as u32,
        }
    }

    fn to_native(self) -> Result<Light, AbiError> {
        if self.enabled > 1 || self.use_color_temperature > 1 {
            return Err("light boolean settings must be 0 or 1".into());
        }
        let kind = LightKind::from_value(self.kind).ok_or("unknown light kind")?;
        let shadows = LightShadowMode::from_value(self.shadows).ok_or("unknown light shadows")?;
        let bake_type = LightBakeType::from_value(self.lightmap_bake_type)
            .ok_or("unknown lightmap bake type")?;
        let light = Light {
            enabled: self.enabled == 1,
            settings: LightSettings {
                kind,
                color: [self.r, self.g, self.b, self.a],
                intensity: self.intensity,
                range: self.range,
                spot_angle: self.spot_angle,
                inner_spot_angle: self.inner_spot_angle,
                shadows,
                shadow_strength: self.shadow_strength,
                shadow_bias: self.shadow_bias,
                shadow_normal_bias: self.shadow_normal_bias,
                shadow_near_plane: self.shadow_near_plane,
                culling_mask: self.culling_mask,
                rendering_layer_mask: self.rendering_layer_mask,
                bounce_intensity: self.bounce_intensity,
                color_temperature: self.color_temperature,
                use_color_temperature: self.use_color_temperature == 1,
                cookie_size: self.cookie_size,
                cookie_texture: self.cookie_texture,
                bake_type,
            },
        };
        Ok(light.normalized()?)
    }
}

fn light(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, Light), AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::Light(light) => Ok((id, light)),
        _ => Err("component is not a Light".into()),
    }
}

/// Creates a light with the engine's default settings on a live object.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_light_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    match crate::physics::create(world, object, ComponentDesc::Light(Light::default())) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

/// # Safety
/// Output is writable for one Light record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_light_read(
    world: u64,
    handle: u64,
    output: *mut DisunityLight,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = light(s, handle)?;
        unsafe { *output = DisunityLight::from_native(value) };
        Ok(())
    })
}

/// Validates every setting before changing the light; a rejected write changes nothing.
/// # Safety
/// Value is readable for one Light record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_light_write(
    world: u64,
    handle: u64,
    value: *const DisunityLight,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, _) = light(s, handle)?;
        s.native
            .set_component(id, ComponentDesc::Light(value.to_native()?))?;
        Ok(())
    })
}
