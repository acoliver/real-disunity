//! Font faces for dynamic glyph atlases: a store of faces keyed by an id, each loaded from font
//! file bytes at a point size. It answers face measurements, glyph indexes and metrics, kerning,
//! and renders glyphs into a caller's atlas (see `faces_render`). The measurements come from the
//! font's tables where it has them; where it has not, the fallbacks below are the engine's choice.
//! Lengths are in pixels at the face's current point size, with the baseline at 0 and up positive.

use super::UiError;
use ttf_parser::{Face, GlyphId, name_id};

/// Identity of a loaded face within one [`FaceBook`]. Ids are never reused by the same book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaceId(pub u32);

/// What a face store accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceLimits {
    pub max_faces: usize,
    pub max_face_bytes: usize,
    pub max_total_bytes: usize,
}
impl Default for FaceLimits {
    fn default() -> Self {
        Self {
            max_faces: 256,
            max_face_bytes: 64 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
        }
    }
}

/// The largest point size a face accepts.
pub const MAX_POINT_SIZE: f32 = 4096.;

pub(super) struct LoadedFace {
    pub(super) data: Vec<u8>,
    pub(super) index: u32,
    pub(super) point_size: f32,
}

/// Face-wide measurements at the current point size.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceInfo {
    pub face_index: u32,
    /// The font's family name (name ID 1), or empty when it has none.
    pub family: String,
    /// The font's style name (name ID 2), or empty when it has none.
    pub style: String,
    pub point_size: f32,
    /// Always 1: lengths are already in pixels at the point size.
    pub scale: f32,
    pub units_per_em: u32,
    /// Ascent minus descent plus the font's line gap.
    pub line_height: f32,
    pub ascent_line: f32,
    /// The font's capital height, or 0.7 em.
    pub cap_line: f32,
    /// The font's x-height, or 0.5 em.
    pub mean_line: f32,
    pub baseline: f32,
    /// Negative below the baseline.
    pub descent_line: f32,
    /// Raise of superscript text; the font's value, or 0.4 em.
    pub superscript_offset: f32,
    /// Size of superscript text as a fraction of the normal size; the font's value, or 0.65.
    pub superscript_size: f32,
    /// Raise of subscript text, negative below the baseline; the font's value, or -0.14 em.
    pub subscript_offset: f32,
    /// Size of subscript text as a fraction of the normal size; the font's value, or 0.65.
    pub subscript_size: f32,
    /// Position of the underline's centre, negative below the baseline; the font's value, or -0.1 em.
    pub underline_offset: f32,
    pub underline_thickness: f32,
    /// Position of the strikethrough's centre; the font's value, or 0.3 em.
    pub strikethrough_offset: f32,
    pub strikethrough_thickness: f32,
    /// The advance of a space, or half an em when the font has no space.
    pub tab_width: f32,
}

/// A glyph's size and placement at the current point size. Bearings are of the glyph's bounding
/// box: `bearing_x` from the origin to its left edge, `bearing_y` from the baseline to its top.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphMetrics {
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

/// A kerning adjustment to the advance of `first` when `second` follows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphPair {
    pub first: u32,
    pub second: u32,
    pub x_advance: f32,
}

/// The loaded faces of one world.
#[derive(Default)]
pub struct FaceBook {
    pub(super) faces: std::collections::BTreeMap<FaceId, LoadedFace>,
    next: u32,
    bytes: usize,
    limits: FaceLimits,
}

fn valid_size(point_size: f32) -> bool {
    point_size.is_finite() && point_size > 0. && point_size <= MAX_POINT_SIZE
}

impl FaceBook {
    #[must_use]
    pub fn new(limits: FaceLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// Loads face `index` of the font file (0 for a single font) at `point_size`.
    pub fn load(&mut self, data: Vec<u8>, index: u32, point_size: f32) -> Result<FaceId, UiError> {
        if !valid_size(point_size) || data.len() > self.limits.max_face_bytes {
            return Err(UiError::InvalidFont);
        }
        let total = self.bytes.saturating_add(data.len());
        if self.faces.len() >= self.limits.max_faces || total > self.limits.max_total_bytes {
            return Err(UiError::FontLimit);
        }
        let face = Face::parse(&data, index).map_err(|_| UiError::InvalidFont)?;
        if face.units_per_em() == 0 || face.number_of_glyphs() == 0 {
            return Err(UiError::InvalidFont);
        }
        ab_glyph::FontRef::try_from_slice_and_index(&data, index)
            .map_err(|_| UiError::InvalidFont)?;
        let id = FaceId(self.next);
        self.next = self.next.checked_add(1).ok_or(UiError::FontLimit)?;
        self.faces.insert(
            id,
            LoadedFace {
                data,
                index,
                point_size,
            },
        );
        self.bytes = total;
        Ok(id)
    }

    pub fn unload(&mut self, id: FaceId) -> Result<(), UiError> {
        let face = self.faces.remove(&id).ok_or(UiError::MissingFace)?;
        self.bytes -= face.data.len();
        Ok(())
    }

    pub fn set_size(&mut self, id: FaceId, point_size: f32) -> Result<(), UiError> {
        if !valid_size(point_size) {
            return Err(UiError::InvalidFont);
        }
        self.faces
            .get_mut(&id)
            .ok_or(UiError::MissingFace)?
            .point_size = point_size;
        Ok(())
    }

    pub(super) fn get(&self, id: FaceId) -> Result<&LoadedFace, UiError> {
        self.faces.get(&id).ok_or(UiError::MissingFace)
    }

    /// The glyph index of a code point, 0 when the face has no glyph for it.
    pub fn glyph_index(&self, id: FaceId, code_point: u32) -> Result<u32, UiError> {
        let face = self.get(id)?.parse();
        Ok(char::from_u32(code_point)
            .and_then(|c| face.glyph_index(c))
            .map_or(0, |glyph| u32::from(glyph.0)))
    }

    pub fn glyph_metrics(&self, id: FaceId, glyph: u32) -> Result<GlyphMetrics, UiError> {
        let stored = self.get(id)?;
        let face = stored.parse();
        let glyph = glyph_id(&face, glyph)?;
        let k = stored.units_to_pixels(&face);
        let advance = face.glyph_hor_advance(glyph).map_or(0., f32::from) * k;
        Ok(face.glyph_bounding_box(glyph).map_or(
            GlyphMetrics {
                advance,
                ..GlyphMetrics::default()
            },
            |b| GlyphMetrics {
                width: f32::from(b.x_max - b.x_min) * k,
                height: f32::from(b.y_max - b.y_min) * k,
                bearing_x: f32::from(b.x_min) * k,
                bearing_y: f32::from(b.y_max) * k,
                advance,
            },
        ))
    }

    /// Kerning for every ordered pair of the distinct `glyphs` (in glyph order) whose advance the font's `kern` table changes.
    /// The `GPOS` pair tables are not read, which is the engine's choice, so a font that kerns only
    /// through them has no adjustments.
    pub fn pair_adjustments(&self, id: FaceId, glyphs: &[u32]) -> Result<Vec<GlyphPair>, UiError> {
        let stored = self.get(id)?;
        let face = stored.parse();
        let k = stored.units_to_pixels(&face);
        let Some(kern) = face.tables().kern else {
            return Ok(Vec::new());
        };
        let mut ids: Vec<GlyphId> = glyphs
            .iter()
            .filter_map(|&g| u16::try_from(g).ok().map(GlyphId))
            .collect();
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        let mut pairs = Vec::new();
        for &first in &ids {
            for &second in &ids {
                let units: i32 = kern
                    .subtables
                    .into_iter()
                    .filter(|t| t.horizontal && !t.variable && !t.has_state_machine)
                    .filter_map(|t| t.glyphs_kerning(first, second))
                    .map(i32::from)
                    .sum();
                if units != 0 {
                    pairs.push(GlyphPair {
                        first: u32::from(first.0),
                        second: u32::from(second.0),
                        x_advance: f32::from(i16::try_from(units).unwrap_or(0)) * k,
                    });
                }
            }
        }
        Ok(pairs)
    }

    pub fn face_info(&self, id: FaceId) -> Result<FaceInfo, UiError> {
        let stored = self.get(id)?;
        let face = stored.parse();
        let k = stored.units_to_pixels(&face);
        let em = stored.point_size;
        let px = |units: i16| f32::from(units) * k;
        let name = |wanted| {
            face.names()
                .into_iter()
                .filter(|n| n.name_id == wanted && n.is_unicode())
                .find_map(|n| n.to_string())
                .unwrap_or_default()
        };
        let line = |m: Option<ttf_parser::LineMetrics>, offset: f32, thickness: f32| {
            m.map_or((offset * em, thickness * em), |m| {
                (px(m.position), px(m.thickness))
            })
        };
        let (underline_offset, underline_thickness) = line(face.underline_metrics(), -0.1, 0.05);
        let (strikethrough_offset, strikethrough_thickness) =
            line(face.strikeout_metrics(), 0.3, 0.05);
        let script = |m: Option<ttf_parser::ScriptMetrics>, sign: f32, offset: f32| {
            m.map_or((offset * em, 0.65), |m| {
                let size = f32::from(m.y_size) / f32::from(face.units_per_em());
                (sign * px(m.y_offset), size)
            })
        };
        let (superscript_offset, superscript_size) = script(face.superscript_metrics(), 1., 0.4);
        let (subscript_offset, subscript_size) = script(face.subscript_metrics(), -1., -0.14);
        let ascent = px(face.ascender());
        let descent = px(face.descender());
        Ok(FaceInfo {
            face_index: stored.index,
            family: name(name_id::FAMILY),
            style: name(name_id::SUBFAMILY),
            point_size: em,
            scale: 1.,
            units_per_em: u32::from(face.units_per_em()),
            line_height: ascent - descent + px(face.line_gap()),
            ascent_line: ascent,
            cap_line: face.capital_height().map_or(0.7 * em, px),
            mean_line: face.x_height().map_or(0.5 * em, px),
            baseline: 0.,
            descent_line: descent,
            superscript_offset,
            superscript_size,
            subscript_offset,
            subscript_size,
            underline_offset,
            underline_thickness,
            strikethrough_offset,
            strikethrough_thickness,
            tab_width: face
                .glyph_index(' ')
                .and_then(|g| face.glyph_hor_advance(g))
                .map_or(0.5 * em, |a| f32::from(a) * k),
        })
    }
}

impl LoadedFace {
    pub(super) fn parse(&self) -> Face<'_> {
        Face::parse(&self.data, self.index).expect("face parsed when loaded")
    }

    /// Pixels per font unit at the current point size.
    pub(super) fn units_to_pixels(&self, face: &Face<'_>) -> f32 {
        self.point_size / f32::from(face.units_per_em())
    }
}

pub(super) fn glyph_id(face: &Face<'_>, glyph: u32) -> Result<GlyphId, UiError> {
    u16::try_from(glyph)
        .ok()
        .filter(|&g| g < face.number_of_glyphs())
        .map(GlyphId)
        .ok_or(UiError::InvalidGlyph)
}
