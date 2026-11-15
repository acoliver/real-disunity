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
//! Only 2D properties get a default that a shader names; the player also has array and cube
//! properties, which would need a texture of their own dimension.
//!
//! One default no shader names is the grey cube. A shader program can reflect a cube slot that
//! nothing sets and the shader does not declare, such as the `_BlitCubeTexture` beside the 2D
//! source of the URP blit program (Old World, 180 draws a run). Unity binds its default cube
//! there. The player's data does not give that cube's colour, so it is assumed to be the 2D
//! grey: RGBA (0.5, 0.5, 0.5, 1) in every texel of all six faces.

use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{SourceDimension, SourceTexture};
use worldrunner_unity_serialized::shader::Shader;

use super::ids;

/// `GraphicsFormat` of 8-bit RGBA in linear colour.
const RGBA8_LINEAR: u32 = 8;

/// `TextureDimension` of a 2D texture.
const TEXTURE_2D: i32 = 2;

/// `ShaderPropertyType` of a texture property.
const TEXTURE_PROPERTY: i32 = 4;

/// `ShaderPropertyType` of a colour, vector, float, range and integer property. A shader stores
/// the default of each in the four floats of its declaration: a colour or vector all four, a
/// float or integer the first, a range its default first and then its limits.
const COLOR_PROPERTY: i32 = 0;
const VECTOR_PROPERTY: i32 = 1;
const FLOAT_PROPERTY: i32 = 2;
const RANGE_PROPERTY: i32 = 3;
const INT_PROPERTY: i32 = 5;

/// A default texture the host supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefaultTexture {
    White,
    Black,
    /// Unity's default 2D texture, RGBA (0.5, 0.5, 0.5, 1).
    Grey,
    /// The cube a slot reads that nothing sets and no shader declares: assumed to be the grey of
    /// the 2D default on every face. No shader names it.
    GreyCube,
}

impl DefaultTexture {
    pub const ALL: [Self; 4] = [Self::White, Self::Black, Self::Grey, Self::GreyCube];

    /// The default a shader names, or `None` for a name this host does not supply.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "white" => Some(Self::White),
            "black" => Some(Self::Black),
            "gray" => Some(Self::Grey),
            _ => None,
        }
    }

    /// The name the shader declares it by, empty for a default no shader names.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
            Self::Grey => "gray",
            Self::GreyCube => "",
        }
    }

    /// The texture to register under [`ids::default_texture`].
    #[must_use]
    pub fn texture(self) -> SourceTexture {
        let (dimension, texel) = match self {
            Self::White => (SourceDimension::D2, [255; 4]),
            Self::Black => (SourceDimension::D2, [0, 0, 0, 255]),
            Self::Grey => (SourceDimension::D2, [128, 128, 128, 255]),
            Self::GreyCube => (SourceDimension::Cube, [128, 128, 128, 255]),
        };
        let faces = match dimension {
            SourceDimension::D2 => 1,
            SourceDimension::Cube => 6,
        };
        SourceTexture {
            id: ids::default_texture(self.index()),
            dimension,
            size: [1, 1],
            graphics_format: RGBA8_LINEAR,
            bytes: texel.repeat(faces),
            sampler: None,
        }
    }

    fn index(self) -> u64 {
        match self {
            Self::White => 0,
            Self::Black => 1,
            Self::Grey => 2,
            Self::GreyCube => 3,
        }
    }

    /// The name the frame gives this texture.
    #[must_use]
    pub fn id(self) -> worldrunner_render_program::ids::TextureId {
        ids::default_texture(self.index())
    }
}

/// The 2D texture properties of `shader` with the name of the default each declares, empty for
/// none, whether or not the host supplies it.
#[must_use]
pub fn declared_names(shader: &Shader) -> Vec<(String, String)> {
    shader
        .parsed
        .properties
        .iter()
        .filter(|property| {
            property.kind == TEXTURE_PROPERTY && property.texture_dimension == TEXTURE_2D
        })
        .map(|property| (property.name.clone(), property.default_texture.clone()))
        .collect()
}

/// The 2D texture properties of `shader` whose declared default is one the host supplies, by
/// property name.
#[must_use]
pub fn declared(shader: &Shader) -> Vec<(String, DefaultTexture)> {
    declared_names(shader)
        .into_iter()
        .filter_map(|(name, default)| {
            DefaultTexture::named(&default).map(|texture| (name, texture))
        })
        .collect()
}

/// `value` as an integer, when it is a whole number an `i32` holds.
#[allow(clippy::cast_possible_truncation)] // a whole number inside the range, checked first
fn whole_number(value: f32) -> Option<i32> {
    let limit = 2_147_483_648.0_f32;
    (value.fract() == 0.0 && (-limit..limit).contains(&value)).then_some(value as i32)
}

/// What a shader declares that fills the properties a material does not set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderFill {
    /// Every 2D texture property, whether or not the host supplies its default texture.
    pub textures: Vec<String>,
    /// Every texture property of any dimension. A texture the shader reads but does not declare
    /// reads Unity's white texture when nothing sets it.
    pub declared_textures: Vec<String>,
    /// The default value of each colour, vector, float, range and integer property, in the
    /// order the shader declares them.
    pub values: Vec<(String, PropertyValue)>,
}

/// What `shader` declares for a material that leaves a property unset.
#[must_use]
pub fn declared_fill(shader: &Shader) -> ShaderFill {
    let mut fill = ShaderFill::default();
    for property in &shader.parsed.properties {
        let default = property.default_value;
        let value = match property.kind {
            TEXTURE_PROPERTY => {
                fill.declared_textures.push(property.name.clone());
                if property.texture_dimension == TEXTURE_2D {
                    fill.textures.push(property.name.clone());
                }
                continue;
            }
            COLOR_PROPERTY | VECTOR_PROPERTY => PropertyValue::Vector(default),
            FLOAT_PROPERTY | RANGE_PROPERTY => PropertyValue::Float(default[0]),
            INT_PROPERTY => match whole_number(default[0]) {
                Some(number) => PropertyValue::Int(number),
                None => continue,
            },
            _ => continue,
        };
        fill.values.push((property.name.clone(), value));
    }
    fill
}
