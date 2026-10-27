//! Source-wide interpretation of the three texture slots on each terrain layer.
use super::{
    SceneNormalEncoding, SceneTexture, SceneValidationError, TerrainLayer, TextureColorSpace,
    TextureHandle,
};

/// A texture slot on each layer, independent of its material interpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainLayerTexture {
    AlbedoMetallic,
    NormalRoughness,
    Height,
}

impl TerrainLayerTexture {
    pub const fn handle(self, layer: TerrainLayer) -> TextureHandle {
        match self {
            Self::AlbedoMetallic => layer.albedo_metallic,
            Self::NormalRoughness => layer.normal_roughness,
            Self::Height => layer.height,
        }
    }
    pub const fn index(self) -> usize {
        match self {
            Self::AlbedoMetallic => 0,
            Self::NormalRoughness => 1,
            Self::Height => 2,
        }
    }
}

/// One sampled component, where 0, 1, 2, 3 denote R, G, B, A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLayerChannel {
    pub texture: TerrainLayerTexture,
    pub channel: u8,
}

/// All composed inputs, with no texture conversion or repacking required.
/// Data channels must be linear; alpha of an sRGB texture remains linear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLayerChannelMap {
    pub albedo: [TerrainLayerChannel; 3],
    pub albedo_color_space: TextureColorSpace,
    pub metallic: TerrainLayerChannel,
    pub height: TerrainLayerChannel,
    /// Z is ignored for `TwoChannelXy` and reconstructed from X and Y.
    pub normal: [TerrainLayerChannel; 3],
    pub normal_encoding: SceneNormalEncoding,
    pub roughness: TerrainLayerChannel,
    /// Interpret the selected component as smoothness.
    pub invert_roughness: bool,
}

impl TerrainLayerChannelMap {
    /// The original packing, preserving the layer's normal encoding and color space.
    pub fn packed(encoding: SceneNormalEncoding, color_space: TextureColorSpace) -> Self {
        use TerrainLayerTexture::{AlbedoMetallic, Height, NormalRoughness};
        let channels = |texture| {
            std::array::from_fn(|channel| TerrainLayerChannel {
                texture,
                channel: u8::try_from(channel).expect("three channels"),
            })
        };
        Self {
            albedo: channels(AlbedoMetallic),
            albedo_color_space: color_space,
            metallic: TerrainLayerChannel {
                texture: AlbedoMetallic,
                channel: 3,
            },
            height: TerrainLayerChannel {
                texture: Height,
                channel: 0,
            },
            normal: channels(NormalRoughness),
            normal_encoding: encoding,
            roughness: TerrainLayerChannel {
                texture: NormalRoughness,
                channel: 3,
            },
            invert_roughness: false,
        }
    }

    pub fn validate(
        self,
        layer: TerrainLayer,
        textures: &[SceneTexture],
    ) -> Result<(), SceneValidationError> {
        if self
            .albedo
            .into_iter()
            .chain(self.normal)
            .chain([self.metallic, self.height, self.roughness])
            .any(|c| c.channel > 3)
        {
            return Err(invalid("terrain channel map component out of range"));
        }
        for channel in self.albedo {
            let texture = channel_texture(channel, layer, textures)?;
            if texture.color_space() != self.albedo_color_space {
                return Err(invalid("terrain albedo color space mismatch"));
            }
        }
        let normal_count = if self.normal_encoding == SceneNormalEncoding::TwoChannelXy {
            2
        } else {
            3
        };
        for channel in self.normal[..normal_count].iter().copied().chain([
            self.metallic,
            self.height,
            self.roughness,
        ]) {
            let texture = channel_texture(channel, layer, textures)?;
            if texture.color_space() != TextureColorSpace::Linear && channel.channel != 3 {
                return Err(invalid("terrain data channels must be linear"));
            }
        }
        Ok(())
    }
}

fn channel_texture(
    channel: TerrainLayerChannel,
    layer: TerrainLayer,
    textures: &[SceneTexture],
) -> Result<&SceneTexture, SceneValidationError> {
    let handle = channel.texture.handle(layer);
    let texture =
        textures
            .get(handle.raw() as usize)
            .ok_or(SceneValidationError::HandleOutOfRange {
                part: "terrain channel map",
                resource: "texture",
                handle: handle.raw(),
                limit: textures.len() as u64,
            })?;
    texture.validate()?;
    if u32::from(channel.channel) >= channel_count(texture)? {
        return Err(invalid("terrain channel map component out of range"));
    }
    Ok(texture)
}

pub(super) fn channel_count(texture: &SceneTexture) -> Result<u32, SceneValidationError> {
    match texture {
        SceneTexture::Sampled { texture, .. } => channel_count(texture),
        SceneTexture::Texels { format, .. } => {
            Ok(u32::try_from(format.bytes_per_texel()).expect("four channels"))
        }
        SceneTexture::Encoded {
            encoding: super::TextureEncoding::DdsContainer,
            bytes,
            color_space,
        } => dds_channels(bytes, *color_space),
        SceneTexture::Encoded { .. } => Err(invalid("terrain layered texture format")),
    }
}

fn dds_channels(bytes: &[u8], declared: TextureColorSpace) -> Result<u32, SceneValidationError> {
    if bytes.get(..4) != Some(b"DDS ") || bytes.get(84..88) != Some(b"DX10") {
        return Err(invalid("terrain channel map requires DX10 DDS"));
    }
    let format = bytes.get(128..132).ok_or(invalid("terrain DDS header"))?;
    let code = u32::from_le_bytes(format.try_into().expect("four bytes"));
    if matches!(code, 29 | 72 | 78 | 91) != (declared == TextureColorSpace::Srgb) {
        return Err(invalid("terrain DDS color space mismatch"));
    }
    match code {
        61 | 80 => Ok(1),
        49 | 83 => Ok(2),
        28 | 29 | 71 | 72 | 77 | 78 | 87 | 91 => Ok(4),
        _ => Err(invalid("terrain layered texture format")),
    }
}

fn invalid(part: &'static str) -> SceneValidationError {
    SceneValidationError::UnsupportedInput { part }
}
