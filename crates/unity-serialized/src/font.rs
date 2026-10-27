//! The `Font` layout: a font file with its glyph atlas, kerning and fallbacks.
//!
//! The font file itself, usually TrueType or OpenType, is returned as a range within the object.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// One glyph's place in a font's texture and how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterInfo {
    /// The character's code point.
    pub index: u32,
    /// The glyph's rectangle in the font texture, in UV space: x, y, width, height.
    pub uv: [f32; 4],
    /// The glyph's quad relative to the pen position: x, y, width, height.
    pub vert: [f32; 4],
    pub advance: f32,
    /// Whether the glyph is stored rotated in the texture.
    pub flipped: bool,
}

/// The fixed fields of a glyph, with the padding after its flag.
const CHARACTER_LEN: usize = 44;

impl CharacterInfo {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let info = Self {
            index: r.u32()?,
            uv: r.floats()?,
            vert: r.floats()?,
            advance: r.f32()?,
            flipped: r.bool()?,
        };
        r.align()?;
        Ok(info)
    }
}

/// The spacing adjustment between two characters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kerning {
    pub first: u16,
    pub second: u16,
    pub adjustment: f32,
}

const KERNING_LEN: usize = 8;

/// Unity's character and line spacing settings for a font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontSpacing {
    pub line_spacing: f32,
    pub tracking: f32,
    pub character_spacing: i32,
    pub character_padding: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub name: String,
    pub default_material: PPtr,
    pub font_size: f32,
    pub texture: PPtr,
    /// The code point of the first glyph in a legacy ASCII-ordered texture.
    pub ascii_start_offset: i32,
    pub spacing: FontSpacing,
    /// Unity's `FontTextureCase`, for example 0 dynamic or 1 Unicode.
    pub convert_case: i32,
    pub characters: Vec<CharacterInfo>,
    pub kerning: Vec<Kerning>,
    pub pixel_scale: f32,
    /// Byte range of the font file within the object; empty when the font has none.
    pub font_data: Range<usize>,
    pub ascent: f32,
    pub descent: f32,
    /// Unity's `FontStyle` flags.
    pub default_style: u32,
    /// The system font names to try, in order.
    pub font_names: Vec<String>,
    pub fallback_fonts: Vec<PPtr>,
    /// Unity's `FontRenderingMode`.
    pub rendering_mode: i32,
    pub use_legacy_bounds_calculation: bool,
    pub should_round_advance_value: bool,
}

/// The length prefix of an empty string.
const STRING_MIN: usize = 4;

impl Layout for Font {
    const CLASS: ClassId = ClassId::FONT;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let line_spacing = r.f32()?;
        let default_material = r.pptr()?;
        let font_size = r.f32()?;
        let texture = r.pptr()?;
        let ascii_start_offset = r.i32()?;
        let spacing = FontSpacing {
            line_spacing,
            tracking: r.f32()?,
            character_spacing: r.i32()?,
            character_padding: r.i32()?,
        };
        let convert_case = r.i32()?;
        let characters = r.array("font character", CHARACTER_LEN, CharacterInfo::read)?;
        let kerning = r.array("kerning pair", KERNING_LEN, |r| {
            Ok(Kerning {
                first: r.u16()?,
                second: r.u16()?,
                adjustment: r.f32()?,
            })
        })?;
        let pixel_scale = r.f32()?;
        let font_data = r.byte_range("font data byte")?;
        Ok(Self {
            name,
            default_material,
            font_size,
            texture,
            ascii_start_offset,
            spacing,
            convert_case,
            characters,
            kerning,
            pixel_scale,
            font_data,
            ascent: r.f32()?,
            descent: r.f32()?,
            default_style: r.u32()?,
            font_names: r.array("font name", STRING_MIN, ObjectReader::string)?,
            fallback_fonts: r.pptrs("fallback font")?,
            rendering_mode: r.i32()?,
            use_legacy_bounds_calculation: r.bool()?,
            should_round_advance_value: r.bool()?,
        })
    }
}
