//! The textures a shader names as the default of a texture property.
//!
//! A shader declares each texture property with the name of a default, and a material that sets
//! no texture for the property reads that default. In the Old World player (Unity 6000.3.15f1)
//! the declared names are `white` (221 properties), `black` (23), `bump` (18), `grey` (12),
//! `linearGrey` (1) and the empty name (43). Two have a colour their name fixes: `white` is every
//! channel at 1, and `black` has every colour channel at 0. Both are 1x1 here, with alpha at 1,
//! which is an assumption: nothing in the player's data or our runs shows the alpha of `black`.
//! `grey`, `linearGrey` and `bump` name colours and colour spaces that the player's data does not
//! give either, so they are not supplied and a draw that reads one is left out and counted.
//! Only 2D properties get a default; the player also has array and cube properties, which would
//! need a texture of their own dimension.

use worldrunner_render_program::offscreen::frame::SourceTexture;
use worldrunner_unity_serialized::shader::Shader;

use super::ids;

/// `GraphicsFormat` of 8-bit RGBA in linear colour.
const RGBA8_LINEAR: u32 = 8;

/// `TextureDimension` of a 2D texture.
const TEXTURE_2D: i32 = 2;

/// `ShaderPropertyType` of a texture property.
const TEXTURE_PROPERTY: i32 = 4;

/// A default texture the host supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefaultTexture {
    White,
    Black,
}

impl DefaultTexture {
    pub const ALL: [Self; 2] = [Self::White, Self::Black];

    /// The default a shader names, or `None` for a name this host does not supply.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "white" => Some(Self::White),
            "black" => Some(Self::Black),
            _ => None,
        }
    }

    /// The name the shader declares it by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
        }
    }

    /// The texture to register under [`ids::default_texture`].
    #[must_use]
    pub fn texture(self) -> SourceTexture {
        let texel = match self {
            Self::White => [255; 4],
            Self::Black => [0, 0, 0, 255],
        };
        SourceTexture {
            id: ids::default_texture(self.index()),
            size: [1, 1],
            graphics_format: RGBA8_LINEAR,
            bytes: texel.to_vec(),
            sampler: None,
        }
    }

    fn index(self) -> u64 {
        match self {
            Self::White => 0,
            Self::Black => 1,
        }
    }

    /// The name the frame gives this texture.
    #[must_use]
    pub fn id(self) -> worldrunner_render_program::ids::TextureId {
        ids::default_texture(self.index())
    }
}

/// The 2D texture properties of `shader` whose declared default is one the host supplies, by
/// property name.
#[must_use]
pub fn declared(shader: &Shader) -> Vec<(String, DefaultTexture)> {
    shader
        .parsed
        .properties
        .iter()
        .filter(|property| {
            property.kind == TEXTURE_PROPERTY && property.texture_dimension == TEXTURE_2D
        })
        .filter_map(|property| {
            DefaultTexture::named(&property.default_texture)
                .map(|texture| (property.name.clone(), texture))
        })
        .collect()
}
