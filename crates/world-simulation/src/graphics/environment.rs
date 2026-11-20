//! The environment lighting of a world: the ambient light and the default reflection.
//!
//! A scene load fills the record from the scene's render settings and a script that writes
//! `RenderSettings` changes the same record, so the world holds one copy. The world draws
//! nothing: a host that draws reads the record from the graphics snapshot and turns it into the
//! shader values a draw reads (the reflection cube, its decode and the spherical harmonics of the
//! ambient light).
//!
//! Each section is `None` until a scene or a script writes it. A host treats a section nobody
//! wrote as having no opinion and keeps its own default for the values it covers, so a world that
//! never sets an environment draws as it did before the record existed. A read of a section that
//! nobody wrote gives the defaults of a new scene ([`Ambient::default`], [`Reflection::default`]).

use super::{GraphicsError, TextureRef};
use crate::{World, WorldError};

/// How many coefficients an ambient probe holds: nine for each of red, green and blue.
pub const PROBE_COEFFICIENTS: usize = 27;

/// Where the ambient light of a scene comes from. The values are the numbers a scene stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AmbientMode {
    /// The probe the scene baked from its skybox.
    #[default]
    Skybox = 0,
    /// A sky, an equator and a ground colour.
    Trilight = 1,
    /// One colour.
    Flat = 3,
    /// A probe a script set.
    Custom = 4,
}

impl AmbientMode {
    /// The mode a scene's number names.
    #[must_use]
    pub fn from_value(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Skybox),
            1 => Some(Self::Trilight),
            3 => Some(Self::Flat),
            4 => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Where the default reflection comes from. The values are the numbers a scene stores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReflectionMode {
    /// The cube baked from the scene's skybox.
    #[default]
    Skybox = 0,
    /// The cube a scene or a script named.
    Custom = 1,
}

impl ReflectionMode {
    /// The mode a scene's number names.
    #[must_use]
    pub fn from_value(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Skybox),
            1 => Some(Self::Custom),
            _ => None,
        }
    }
}

/// The ambient light. A colour is linear RGBA. `probe` holds the nine coefficients of red, then
/// of green, then of blue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ambient {
    pub mode: AmbientMode,
    pub sky: [f32; 4],
    pub equator: [f32; 4],
    pub ground: [f32; 4],
    /// The colour of the flat mode.
    pub flat: [f32; 4],
    pub intensity: f32,
    pub probe: [f32; PROBE_COEFFICIENTS],
}

impl Default for Ambient {
    /// The ambient light of a new scene.
    fn default() -> Self {
        let sky = [0.212, 0.227, 0.259, 1.0];
        Self {
            mode: AmbientMode::Skybox,
            sky,
            equator: [0.114, 0.125, 0.133, 1.0],
            ground: [0.047, 0.043, 0.035, 1.0],
            flat: sky,
            intensity: 1.0,
            probe: [0.0; PROBE_COEFFICIENTS],
        }
    }
}

/// The default reflection. A source is a stored cube by its identity, a render texture of the
/// world, or no texture.
#[derive(Clone, Debug, PartialEq)]
pub struct Reflection {
    pub mode: ReflectionMode,
    pub intensity: f32,
    /// The cube a custom reflection reads.
    pub custom: TextureRef,
    /// The cube baked from the skybox, which a skybox reflection reads.
    pub skybox: TextureRef,
}

impl Default for Reflection {
    /// The reflection of a new scene: the skybox's cube, which none is set for, at full strength.
    fn default() -> Self {
        Self {
            mode: ReflectionMode::Skybox,
            intensity: 1.0,
            custom: TextureRef::None,
            skybox: TextureRef::None,
        }
    }
}

impl Reflection {
    /// The texture a draw reads as the scene's reflection: the custom cube in the custom mode and
    /// the skybox's cube in the skybox mode. The other mode's cube is not read, and a mode whose
    /// cube is not set reads none.
    #[must_use]
    pub fn source(&self) -> &TextureRef {
        match self.mode {
            ReflectionMode::Custom => &self.custom,
            ReflectionMode::Skybox => &self.skybox,
        }
    }
}

/// What a world holds of its scene's environment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Environment {
    pub ambient: Option<Ambient>,
    pub reflection: Option<Reflection>,
}

impl Environment {
    /// An environment nobody has written.
    pub(crate) const fn new() -> Self {
        Self {
            ambient: None,
            reflection: None,
        }
    }
}

fn finite(values: &[f32]) -> Result<(), GraphicsError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(GraphicsError::NotFinite)
    }
}

impl Ambient {
    fn validate(&self) -> Result<(), GraphicsError> {
        for colour in [&self.sky, &self.equator, &self.ground, &self.flat] {
            finite(colour)?;
        }
        finite(&self.probe)?;
        finite(&[self.intensity])?;
        if self.intensity < 0.0 {
            return Err(GraphicsError::InvalidEnvironment);
        }
        Ok(())
    }
}

impl World {
    /// The environment the scene or a script set.
    #[must_use]
    pub fn environment(&self) -> &Environment {
        &self.graphics.environment
    }

    /// Replaces the environment. A section of `None` goes back to unset.
    ///
    /// # Errors
    /// Fails when a value is not finite, an intensity is negative, or a reflection source is a
    /// texture the world cannot name: an asset of identity 0, a render texture that is gone, or
    /// a global or temporary texture. The environment stays as it was.
    pub fn set_environment(&mut self, environment: Environment) -> Result<(), WorldError> {
        if let Some(ambient) = &environment.ambient {
            ambient.validate()?;
        }
        if let Some(reflection) = &environment.reflection {
            finite(&[reflection.intensity])?;
            if reflection.intensity < 0.0 {
                return Err(GraphicsError::InvalidEnvironment.into());
            }
            self.check_texture(&reflection.custom)?;
            self.check_texture(&reflection.skybox)?;
        }
        self.graphics.environment = environment;
        Ok(())
    }

    /// The environment with a render texture that was destroyed read as no texture, as a host
    /// that draws sees it.
    pub(crate) fn live_environment(&self) -> Environment {
        let mut environment = self.graphics.environment.clone();
        if let Some(reflection) = &mut environment.reflection {
            reflection.custom = self.live_texture(reflection.custom.clone());
            reflection.skybox = self.live_texture(reflection.skybox.clone());
        }
        environment
    }
}
