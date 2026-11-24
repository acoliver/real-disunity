//! The textures a shader names as the default of a texture property.
//!
//! A shader declares each texture property with the name of a default and a dimension, and a
//! material that sets no texture for the property reads that default, in the property's
//! dimension. In the Old World player (Unity 6000.3.15f1) the declared names are `white` (221
//! properties), `black` (23), `bump` (18), `grey` (12), `linearGrey` (1) and the empty name (43).
//!
//! The recognised names are `white`, `black`, `gray`, `grey`, `linearGrey` and `bump`. They are
//! matched exactly as written, the names as the shaders declare them, so `White` is not a
//! default. The empty name and any other name are not a default this host supplies: a property
//! declared with one stays missing, and the draw is left out and counted. The texels are this
//! engine's choices:
//!
//! * `white` is every channel at 1 and `black` has every colour channel at 0, alpha 1 for both.
//!   The alpha of `black` is an assumption: nothing in the player's data or our runs shows it.
//! * `gray`, `grey` and `linearGrey` are one texel, RGBA (0.5, 0.5, 0.5, 1), as bytes
//!   (128, 128, 128, 255). The host uploads every default as 8-bit linear (`GraphicsFormat` 8), so
//!   a sampled grey is 0.5 for each name. Whether `linearGrey` should sample differently is not
//!   measured.
//! * `bump` is RGBA (0.5, 0.5, 1, 0.5), as bytes (128, 128, 255, 128). Its RGB is the flat
//!   tangent-space normal under the `2 * rgb - 1` decode, which gives (0, 0, 1). Its alpha is not
//!   established: how the game's shaders read the alpha of an unset normal map is not known here
//!   and is to be settled from what game runs show. 0.5 is stored as 128, the nearest byte
//!   (127.5 rounds up).
//!
//! A default exists in the four dimensions a draw can sample, the property's own declared
//! dimension in the serialized shader data: 2D, cube (the texel on all six faces), 2D array (one
//! layer) and 3D (one slice). A cube-array property gets none: the offscreen state uploads no
//! cube-array source, so such a declared default stays missing. The four colours and four shapes
//! make one table of 16 textures, and each is nameable by a shader.
//!
//! Two rows serve slots beyond the declared ones. A shader program can reflect a cube slot that
//! nothing sets and the shader does not declare, such as the `_BlitCubeTexture` beside the 2D
//! source of the URP blit program (Old World, 180 draws a run), and a 3D slot likewise, such as
//! `_3DNoiseTexture`. Each reads a default of its own dimension. The player's data does not give
//! their colour, so each is assumed to be the 2D grey: RGBA (0.5, 0.5, 0.5, 1) in every texel of
//! all six faces, or in the one texel. They are the (grey, cube) and (grey, 3D) rows, the same
//! textures a declared `grey` cube or volume reads.

use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{SourceDimension, SourceTexture};
use worldrunner_unity_serialized::shader::Shader;

use super::ids;

/// `GraphicsFormat` of 8-bit RGBA in linear colour.
const RGBA8_LINEAR: u32 = 8;

/// The texture dimension codes of a property's declaration: a 2D texture, a 3D texture, a cube and
/// a 2D array.
const TEXTURE_2D: i32 = 2;
const TEXTURE_3D: i32 = 3;
const TEXTURE_CUBE: i32 = 4;
const TEXTURE_2D_ARRAY: i32 = 5;

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

/// How the stand-in textures of the host look. Magenta (255, 0, 255, 255) in each dimension shows
/// in a capture where stand-ins are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StandInLook {
    /// Each default has the colour its name stands for.
    #[default]
    Declared,
    /// Every default is magenta, whatever its name.
    Magenta,
}

impl StandInLook {
    /// The look the name `declared` or `magenta` stands for.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "declared" => Some(Self::Declared),
            "magenta" => Some(Self::Magenta),
            _ => None,
        }
    }

    /// The texel a default of `colour` holds under this look, in 8-bit RGBA.
    fn texel(self, colour: Colour) -> [u8; 4] {
        match self {
            Self::Declared => colour.texel(),
            Self::Magenta => [255, 0, 255, 255],
        }
    }
}

/// What a default texture holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Colour {
    White,
    Black,
    Grey,
    Bump,
}

impl Colour {
    /// The colour a shader's default name stands for.
    fn named(name: &str) -> Option<Self> {
        match name {
            "white" => Some(Self::White),
            "black" => Some(Self::Black),
            "gray" | "grey" | "linearGrey" => Some(Self::Grey),
            "bump" => Some(Self::Bump),
            _ => None,
        }
    }

    /// The name the shader declares it by, the first of its spellings.
    fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Black => "black",
            Self::Grey => "gray",
            Self::Bump => "bump",
        }
    }

    /// The texel, in 8-bit RGBA.
    fn texel(self) -> [u8; 4] {
        match self {
            Self::White => [255; 4],
            Self::Black => [0, 0, 0, 255],
            Self::Grey => [128, 128, 128, 255],
            Self::Bump => [128, 128, 255, 128],
        }
    }
}

/// The dimension of a default texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    D2,
    Cube,
    D2Array,
    /// A volume one slice deep.
    D3,
}

impl Shape {
    /// The shape of a property declared with the dimension `code`, if a draw can sample a default
    /// of it.
    fn of_dimension(code: i32) -> Option<Self> {
        match code {
            TEXTURE_2D => Some(Self::D2),
            TEXTURE_CUBE => Some(Self::Cube),
            TEXTURE_2D_ARRAY => Some(Self::D2Array),
            TEXTURE_3D => Some(Self::D3),
            _ => None,
        }
    }

    /// The dimension code of the shape.
    fn dimension(self) -> i32 {
        match self {
            Self::D2 => TEXTURE_2D,
            Self::Cube => TEXTURE_CUBE,
            Self::D2Array => TEXTURE_2D_ARRAY,
            Self::D3 => TEXTURE_3D,
        }
    }

    /// The upload of the shape and the number of texels it holds: six faces, or one layer or
    /// slice.
    fn source(self) -> (SourceDimension, usize) {
        match self {
            Self::D2 => (SourceDimension::D2, 1),
            Self::Cube => (SourceDimension::Cube, 6),
            Self::D2Array => (SourceDimension::D2Array(1), 1),
            Self::D3 => (SourceDimension::D3(1), 1),
        }
    }
}

/// A default texture the host supplies: one colour in one shape. The name of a variant is the
/// colour and the shape, none for 2D.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefaultTexture {
    White,
    Black,
    /// The 2D grey, RGBA (0.5, 0.5, 0.5, 1).
    Grey,
    /// The grey on each of six faces: the default of a declared cube named `grey`, and the one a
    /// cube slot reads that nothing sets and no shader declares.
    GreyCube,
    Bump,
    WhiteCube,
    BlackCube,
    BumpCube,
    WhiteArray,
    BlackArray,
    GreyArray,
    BumpArray,
    White3d,
    Black3d,
    /// The grey volume: the default of a declared 3D property named `grey`, and the one a 3D slot
    /// reads that nothing sets and no shader declares.
    Grey3d,
    Bump3d,
}

impl DefaultTexture {
    /// Every default, in the order the variants are declared, which numbers them: a texture is
    /// registered under [`ids::default_texture`] of its place here, which has room for sixteen.
    pub const ALL: [Self; 16] = [
        Self::White,
        Self::Black,
        Self::Grey,
        Self::GreyCube,
        Self::Bump,
        Self::WhiteCube,
        Self::BlackCube,
        Self::BumpCube,
        Self::WhiteArray,
        Self::BlackArray,
        Self::GreyArray,
        Self::BumpArray,
        Self::White3d,
        Self::Black3d,
        Self::Grey3d,
        Self::Bump3d,
    ];

    /// The one table: what each default holds and in which shape.
    fn parts(self) -> (Colour, Shape) {
        match self {
            Self::White => (Colour::White, Shape::D2),
            Self::Black => (Colour::Black, Shape::D2),
            Self::Grey => (Colour::Grey, Shape::D2),
            Self::Bump => (Colour::Bump, Shape::D2),
            Self::WhiteCube => (Colour::White, Shape::Cube),
            Self::BlackCube => (Colour::Black, Shape::Cube),
            Self::GreyCube => (Colour::Grey, Shape::Cube),
            Self::BumpCube => (Colour::Bump, Shape::Cube),
            Self::WhiteArray => (Colour::White, Shape::D2Array),
            Self::BlackArray => (Colour::Black, Shape::D2Array),
            Self::GreyArray => (Colour::Grey, Shape::D2Array),
            Self::BumpArray => (Colour::Bump, Shape::D2Array),
            Self::White3d => (Colour::White, Shape::D3),
            Self::Black3d => (Colour::Black, Shape::D3),
            Self::Grey3d => (Colour::Grey, Shape::D3),
            Self::Bump3d => (Colour::Bump, Shape::D3),
        }
    }

    /// The default a shader names for a property of the declared `dimension` code, or `None` for a
    /// name or a dimension this host does not supply.
    #[must_use]
    pub fn named(name: &str, dimension: i32) -> Option<Self> {
        let wanted = (Colour::named(name)?, Shape::of_dimension(dimension)?);
        Self::ALL
            .into_iter()
            .find(|texture| texture.parts() == wanted)
    }

    /// The name the shader declares it by.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.parts().0.name()
    }

    /// The dimension code of the default.
    #[must_use]
    pub fn dimension(self) -> i32 {
        self.parts().1.dimension()
    }

    /// The texture to register under [`ids::default_texture`] when the host's stand-ins look as
    /// `look` says.
    #[must_use]
    pub fn texture(self, look: StandInLook) -> SourceTexture {
        let (colour, shape) = self.parts();
        let (dimension, texels) = shape.source();
        SourceTexture {
            id: self.id(),
            dimension,
            size: [1, 1],
            graphics_format: RGBA8_LINEAR,
            bytes: look.texel(colour).repeat(texels),
            mips: 1,
            sampler: None,
        }
    }

    /// The name the frame gives this texture.
    #[must_use]
    pub fn id(self) -> worldrunner_render_program::ids::TextureId {
        // The variants are declared in the order of `ALL`, so the discriminant is the place.
        ids::default_texture(self as u64)
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

/// The texture properties of `shader`, of any dimension, whose declared default is one the host
/// supplies in the dimension the property declares, by property name.
#[must_use]
pub fn declared(shader: &Shader) -> Vec<(String, DefaultTexture)> {
    shader
        .parsed
        .properties
        .iter()
        .filter(|property| property.kind == TEXTURE_PROPERTY)
        .filter_map(|property| {
            DefaultTexture::named(&property.default_texture, property.texture_dimension)
                .map(|texture| (property.name.clone(), texture))
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
    /// Every texture property of any dimension. A texture the shader reads but does not list
    /// here has no default of its own, and this engine gives it the grey default when nothing
    /// sets it.
    pub declared_textures: Vec<String>,
    /// The default value of each colour, vector, float, range and integer property, in the
    /// order the shader declares them.
    pub values: Vec<(String, PropertyValue)>,
    /// The names among `values` that are colour properties. A default of one is authored in
    /// gamma space, and a vector property's is not a colour.
    pub colours: Vec<String>,
}

impl ShaderFill {
    /// This fill for a project in Linear colour space when `linear`: each colour property's
    /// default takes its linear form, RGB only, as the managed property store converts a colour
    /// a game sets (`PropertyStoreColors`). A vector default and every other value stay as
    /// declared, and a Gamma project's fill is unchanged.
    #[must_use]
    pub fn in_colour_space(mut self, linear: bool) -> Self {
        if !linear {
            return self;
        }
        for (name, value) in &mut self.values {
            if let PropertyValue::Vector([red, green, blue, _]) = value
                && self.colours.contains(name)
            {
                for channel in [red, green, blue] {
                    *channel = gamma_to_linear(*channel);
                }
            }
        }
        self
    }
}

/// The linear value of a gamma-space colour channel: the sRGB decode, the same function as the
/// managed `Mathf.GammaToLinearSpace`.
fn gamma_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
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
            COLOR_PROPERTY => {
                fill.colours.push(property.name.clone());
                PropertyValue::Vector(default)
            }
            VECTOR_PROPERTY => PropertyValue::Vector(default),
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
