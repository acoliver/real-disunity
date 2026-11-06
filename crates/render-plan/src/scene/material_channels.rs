use super::{
    SceneImpostorChannels, SceneMaterial, SceneTexture, SceneValidationError, TexelFormat,
    TextureEncoding, TextureHandle,
};

/// A stored scalar component, before any material factor or lighting operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TextureChannel {
    R = 0,
    G = 1,
    B = 2,
    A = 3,
}

impl TryFrom<u32> for TextureChannel {
    type Error = SceneValidationError;

    fn try_from(observed: u32) -> Result<Self, Self::Error> {
        match observed {
            0 => Ok(Self::R),
            1 => Ok(Self::G),
            2 => Ok(Self::B),
            3 => Ok(Self::A),
            _ => Err(SceneValidationError::InvalidTextureChannel { observed }),
        }
    }
}

/// Scalar map layout for mesh surfaces and material-mode impostors.
/// Roughness/metalness share a map; occlusion and emission each have their own.
/// Translucency masks reinterpret the selected metalness component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneMaterialChannels {
    pub roughness: TextureChannel,
    pub metalness: TextureChannel,
    pub ambient_occlusion: TextureChannel,
    pub self_illumination: TextureChannel,
}

impl Default for SceneMaterialChannels {
    fn default() -> Self {
        Self {
            roughness: TextureChannel::G,
            metalness: TextureChannel::B,
            ambient_occlusion: TextureChannel::R,
            self_illumination: TextureChannel::R,
        }
    }
}

impl SceneMaterialChannels {
    /// Packed impostor RT uses R/G; emission/occlusion shares the second map's R/G.
    pub const PACKED_IMPOSTOR: Self = Self {
        roughness: TextureChannel::R,
        metalness: TextureChannel::G,
        ambient_occlusion: TextureChannel::G,
        self_illumination: TextureChannel::R,
    };
}

impl SceneMaterial {
    /// Selects mesh scalar lanes without changing the source texture or its compression.
    /// Scalars may share a stored lane. Resource admission checks format support.
    pub fn with_channels(
        mut self,
        channels: SceneMaterialChannels,
    ) -> Result<Self, SceneValidationError> {
        self.channels = channels;
        Ok(self)
    }

    pub const fn channels(&self) -> SceneMaterialChannels {
        self.channels
    }

    pub(super) fn require_default_channels(
        &self,
        part: &'static str,
    ) -> Result<(), SceneValidationError> {
        if self.channels() != SceneMaterialChannels::default() {
            return Err(SceneValidationError::UnsupportedMaterialChannels { part });
        }
        Ok(())
    }

    pub(super) fn validate_surface_channels(
        &self,
        textures: &[SceneTexture],
    ) -> Result<(), SceneValidationError> {
        let c = self.channels();
        for (part, handle, channel) in [
            ("material roughness", self.roughness_metallic(), c.roughness),
            ("material metalness", self.roughness_metallic(), c.metalness),
            (
                "material occlusion",
                self.ambient_occlusion(),
                c.ambient_occlusion,
            ),
            (
                "material emission",
                self.self_illumination(),
                c.self_illumination,
            ),
        ] {
            validate_channel(textures, part, handle, channel)?;
        }
        Ok(())
    }

    pub(super) fn validate_scalar_channels(
        &self,
        textures: &[SceneTexture],
    ) -> Result<(), SceneValidationError> {
        if let Some(atlas) = self.impostor().filter(|atlas| {
            atlas.channels() == SceneImpostorChannels::RoughTranslucencyEmissionOcclusion
        }) {
            let c = atlas.material_channels();
            for (part, handle, channel) in [
                ("impostor roughness", self.roughness_metallic(), c.roughness),
                (
                    "impostor metalness/translucency",
                    self.roughness_metallic(),
                    c.metalness,
                ),
                (
                    "impostor occlusion",
                    self.ambient_occlusion(),
                    c.ambient_occlusion,
                ),
                (
                    "impostor emission",
                    self.ambient_occlusion(),
                    c.self_illumination,
                ),
            ] {
                validate_channel(textures, part, handle, channel)?;
            }
        } else {
            self.validate_surface_channels(textures)?;
        }
        Ok(())
    }
}

impl super::RenderScene {
    pub(super) fn validate_mesh_scalar_channels(&self) -> Result<(), SceneValidationError> {
        use super::BillboardPolicy::{Spherical, YawLocked};
        for object in self.geometry().objects() {
            let mesh = &self.geometry().meshes()[object.mesh().raw() as usize];
            for lod in mesh.lods() {
                if matches!(
                    lod.card()
                        .map_or(lod.billboard(), |card| Some(card.policy())),
                    Some(Spherical | YawLocked)
                ) {
                    continue;
                }
                let validate = |handle: super::MaterialHandle| {
                    let material = &self.resources().materials()[handle.raw() as usize];
                    if material.impostor().is_some() {
                        material.validate_surface_channels(self.resources().textures())?;
                    }
                    Ok(())
                };
                if let Some(slots) = lod.material_slots() {
                    for slot in slots {
                        validate(slot.material())?;
                    }
                } else {
                    validate(object.material())?;
                }
            }
        }
        Ok(())
    }
}

fn validate_channel(
    textures: &[SceneTexture],
    part: &'static str,
    handle: Option<TextureHandle>,
    channel: TextureChannel,
) -> Result<(), SceneValidationError> {
    let Some(handle) = handle else {
        return Ok(());
    };
    // Texture references are checked before channel admission.
    let texture = &textures[handle.raw() as usize];
    let count = match texture.payload() {
        SceneTexture::Texels { format, .. } => match format {
            TexelFormat::R8Unorm => 1,
            TexelFormat::Rg8Unorm | TexelFormat::Rg16Float => 2,
            TexelFormat::Rgba8Unorm | TexelFormat::Bgra8Unorm | TexelFormat::Rgba16Float => 4,
        },
        SceneTexture::Encoded {
            encoding: TextureEncoding::DdsContainer,
            bytes,
            ..
        } => worldrunner_texture::TextureFormat::from_dx10_dds(bytes)
            .map_err(|_| SceneValidationError::InvalidMaterialTextureFormat { part })?
            .channel_count(),
        _ => return Ok(()),
    };
    if channel as u32 >= count {
        return Err(SceneValidationError::MissingTextureChannel { part, channel });
    }
    Ok(())
}
