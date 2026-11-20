//! The scene's environment lighting: the ambient light and the default reflection. The world
//! holds one record (see `worldrunner_world_simulation::graphics::environment`); a scene load and
//! `RenderSettings` read and write it through these two calls.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics_properties::{texture_parts, texture_ref},
};
use worldrunner_world_simulation::graphics::environment::{
    Ambient, AmbientMode, Environment, PROBE_COEFFICIENTS, Reflection, ReflectionMode,
};

/// The environment lighting of a world. A section a scene or a script never wrote reads with its
/// `*_set` flag 0 and the values of a new scene; writing a section with its flag 0 puts it back to
/// unwritten, whatever its other fields hold. Colours are linear RGBA. `ambient_mode` is 0 skybox,
/// 1 trilight, 3 flat or 4 custom, and `reflection_mode` 0 skybox or 1 custom. `ambient_probe`
/// holds the nine coefficients of red, then of green, then of blue. A reflection cube is a texture
/// kind and identity as `DisunityTextureSlot` gives them (0 none, 1 a stored texture, 2 a render
/// texture). `padding` is zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityEnvironment {
    pub ambient_set: u32,
    pub ambient_mode: u32,
    pub ambient_intensity: f32,
    pub reflection_set: u32,
    pub reflection_mode: u32,
    pub reflection_intensity: f32,
    pub custom_kind: u32,
    pub skybox_kind: u32,
    pub custom_texture: u64,
    pub skybox_texture: u64,
    pub ambient_sky: [f32; 4],
    pub ambient_equator: [f32; 4],
    pub ambient_ground: [f32; 4],
    pub ambient_flat: [f32; 4],
    pub ambient_probe: [f32; 27],
    pub padding: u32,
}

const _: () = assert!(27 == PROBE_COEFFICIENTS);

impl DisunityEnvironment {
    /// The record of `environment`.
    fn from_native(environment: &Environment) -> Self {
        let ambient = environment.ambient.unwrap_or_default();
        let reflection = environment.reflection.clone().unwrap_or_default();
        let (custom_kind, custom_texture) = texture_parts(&reflection.custom);
        let (skybox_kind, skybox_texture) = texture_parts(&reflection.skybox);
        Self {
            ambient_set: u32::from(environment.ambient.is_some()),
            ambient_mode: ambient.mode as u32,
            ambient_intensity: ambient.intensity,
            reflection_set: u32::from(environment.reflection.is_some()),
            reflection_mode: reflection.mode as u32,
            reflection_intensity: reflection.intensity,
            custom_kind,
            skybox_kind,
            custom_texture,
            skybox_texture,
            ambient_sky: ambient.sky,
            ambient_equator: ambient.equator,
            ambient_ground: ambient.ground,
            ambient_flat: ambient.flat,
            ambient_probe: ambient.probe,
            padding: 0,
        }
    }

    fn ambient(&self) -> Result<Option<Ambient>, AbiError> {
        match self.ambient_set {
            0 => Ok(None),
            1 => Ok(Some(Ambient {
                mode: AmbientMode::from_value(self.ambient_mode)
                    .ok_or("an ambient mode is 0 (skybox), 1 (trilight), 3 (flat) or 4 (custom)")?,
                sky: self.ambient_sky,
                equator: self.ambient_equator,
                ground: self.ambient_ground,
                flat: self.ambient_flat,
                intensity: self.ambient_intensity,
                probe: self.ambient_probe,
            })),
            _ => Err("an environment section is set (1) or not (0)".into()),
        }
    }

    fn reflection(&self) -> Result<Option<Reflection>, AbiError> {
        match self.reflection_set {
            0 => Ok(None),
            1 => Ok(Some(Reflection {
                mode: ReflectionMode::from_value(self.reflection_mode)
                    .ok_or("a reflection mode is 0 (skybox) or 1 (custom)")?,
                intensity: self.reflection_intensity,
                custom: texture_ref(self.custom_kind, self.custom_texture)?,
                skybox: texture_ref(self.skybox_kind, self.skybox_texture)?,
            })),
            _ => Err("an environment section is set (1) or not (0)".into()),
        }
    }

    fn to_native(self) -> Result<Environment, AbiError> {
        if self.padding != 0 {
            return Err("the padding of an environment record is zero".into());
        }
        Ok(Environment {
            ambient: self.ambient()?,
            reflection: self.reflection()?,
        })
    }
}

/// Reads the world's environment into `output`.
///
/// # Safety
/// `output` must be valid for one write of a `DisunityEnvironment`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_environment_read(
    world: u64,
    output: *mut DisunityEnvironment,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let store = crate::lock_store();
    let Ok(session) = store.session(world) else {
        return DisunityResult::invalid_handle();
    };
    unsafe { *output = DisunityEnvironment::from_native(session.native.environment()) };
    DisunityResult::ok()
}

/// Replaces the world's environment with `input`. A value that is not finite, an intensity below
/// zero, a mode that is none of the listed ones or a texture the world cannot name is refused and
/// the environment stays as it was.
///
/// # Safety
/// `input` must be valid for one read of a `DisunityEnvironment`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_environment_write(
    world: u64,
    input: *const DisunityEnvironment,
) -> DisunityResult {
    if input.is_null() {
        return DisunityResult::invalid_argument();
    }
    let record = unsafe { *input };
    mutate(world, |session| {
        let environment = record.to_native()?;
        session.native.set_environment(environment)?;
        Ok(())
    })
}
