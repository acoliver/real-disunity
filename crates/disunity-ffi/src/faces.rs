//! Font faces for dynamic glyph atlases. The managed engine passes font file bytes and reads face
//! measurements, glyph indexes, metrics and kerning back; the faces and the glyph drawing live in
//! the world simulation's `FaceBook`. Face ids are per world.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError};
use worldrunner_world_simulation::ui::{FaceId, GlyphJob, GlyphRect, RenderMode};

/// The largest font file accepted.
const MAX_FONT_BYTES: u32 = 64 * 1024 * 1024;
/// The largest atlas image accepted, 256 MiB.
const MAX_ATLAS_BYTES: u32 = 1 << 28;

/// Face measurements in pixels at the face's point size.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityFaceInfo {
    pub face_index: u32,
    pub units_per_em: u32,
    pub point_size: f32,
    pub scale: f32,
    pub line_height: f32,
    pub ascent_line: f32,
    pub cap_line: f32,
    pub mean_line: f32,
    pub baseline: f32,
    pub descent_line: f32,
    pub superscript_offset: f32,
    pub superscript_size: f32,
    pub subscript_offset: f32,
    pub subscript_size: f32,
    pub underline_offset: f32,
    pub underline_thickness: f32,
    pub strikethrough_offset: f32,
    pub strikethrough_thickness: f32,
    pub tab_width: f32,
}

/// A glyph's size and placement in pixels at the face's point size.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityGlyphMetrics {
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

/// A change to the advance of `first` when `second` follows it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityGlyphPair {
    pub first: u32,
    pub second: u32,
    pub x_advance: f32,
}

/// One glyph to draw into an atlas. `mode` is 0 for coverage and 1 for a signed distance field.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityGlyphRender {
    pub glyph: u32,
    pub mode: u32,
    pub padding: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub atlas_width: u32,
    pub atlas_height: u32,
}

fn face_of(id: u64) -> Result<FaceId, AbiError> {
    u32::try_from(id)
        .map(FaceId)
        .map_err(|_| "face id is not one this world issued".into())
}

/// Loads face `face_index` of the font file at `point_size` and writes its id.
///
/// # Safety
/// `bytes` is readable for `length` bytes; `out_face` is writable for one id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_load(
    world: u64,
    bytes: *const u8,
    length: u32,
    face_index: u32,
    point_size: f32,
    out_face: *mut u64,
) -> DisunityResult {
    if bytes.is_null() || out_face.is_null() || length == 0 || length > MAX_FONT_BYTES {
        return DisunityResult::invalid_argument();
    }
    let data = unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec();
    mutate(world, |s| {
        let id = s
            .faces
            .load(data, face_index, point_size)
            .map_err(AbiError::from)?;
        unsafe { *out_face = u64::from(id.0) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_face_unload(world: u64, face: u64) -> DisunityResult {
    mutate(world, |s| {
        s.faces.unload(face_of(face)?).map_err(AbiError::from)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn disunity_face_set_size(world: u64, face: u64, point_size: f32) -> DisunityResult {
    mutate(world, |s| {
        s.faces
            .set_size(face_of(face)?, point_size)
            .map_err(AbiError::from)
    })
}

/// # Safety
/// `out` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_info(
    world: u64,
    face: u64,
    out: *mut DisunityFaceInfo,
) -> DisunityResult {
    if out.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let i = s.faces.face_info(face_of(face)?).map_err(AbiError::from)?;
        unsafe {
            *out = DisunityFaceInfo {
                face_index: i.face_index,
                units_per_em: i.units_per_em,
                point_size: i.point_size,
                scale: i.scale,
                line_height: i.line_height,
                ascent_line: i.ascent_line,
                cap_line: i.cap_line,
                mean_line: i.mean_line,
                baseline: i.baseline,
                descent_line: i.descent_line,
                superscript_offset: i.superscript_offset,
                superscript_size: i.superscript_size,
                subscript_offset: i.subscript_offset,
                subscript_size: i.subscript_size,
                underline_offset: i.underline_offset,
                underline_thickness: i.underline_thickness,
                strikethrough_offset: i.strikethrough_offset,
                strikethrough_thickness: i.strikethrough_thickness,
                tab_width: i.tab_width,
            };
        }
        Ok(())
    })
}

/// Writes a face's family name (`which` 0) or style name (`which` 1) as UTF-8 and its length.
/// The bytes are copied only when `capacity` holds them; a zero capacity asks for the length.
///
/// # Safety
/// `bytes` is writable for `capacity` bytes, or null when `capacity` is zero; `length` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_name(
    world: u64,
    face: u64,
    which: u32,
    bytes: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> DisunityResult {
    if length.is_null() || which > 1 || (capacity > 0 && bytes.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let info = s.faces.face_info(face_of(face)?).map_err(AbiError::from)?;
        let name = if which == 0 { info.family } else { info.style };
        let needed = u32::try_from(name.len()).map_err(|_| "face name is too long")?;
        unsafe { *length = needed };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < needed {
            return Err("face name capacity is too small".into());
        }
        unsafe { std::ptr::copy_nonoverlapping(name.as_ptr(), bytes, name.len()) };
        Ok(())
    })
}

/// Writes the glyph index for a code point; 0 means the face has no glyph for it.
///
/// # Safety
/// `out` is writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_glyph_index(
    world: u64,
    face: u64,
    unicode: u32,
    out: *mut u32,
) -> DisunityResult {
    if out.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let glyph = s
            .faces
            .glyph_index(face_of(face)?, unicode)
            .map_err(AbiError::from)?;
        unsafe { *out = glyph };
        Ok(())
    })
}

/// # Safety
/// `out` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_glyph_metrics(
    world: u64,
    face: u64,
    glyph: u32,
    out: *mut DisunityGlyphMetrics,
) -> DisunityResult {
    if out.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let m = s
            .faces
            .glyph_metrics(face_of(face)?, glyph)
            .map_err(AbiError::from)?;
        unsafe {
            *out = DisunityGlyphMetrics {
                width: m.width,
                height: m.height,
                bearing_x: m.bearing_x,
                bearing_y: m.bearing_y,
                advance: m.advance,
            };
        }
        Ok(())
    })
}

/// Draws a glyph into the rows of an atlas image of one byte per pixel, bottom row first.
///
/// # Safety
/// `render` is readable for one record; `atlas` is writable for `atlas_length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_glyph_render(
    world: u64,
    face: u64,
    render: *const DisunityGlyphRender,
    atlas: *mut u8,
    atlas_length: u32,
) -> DisunityResult {
    if render.is_null() || atlas.is_null() || atlas_length > MAX_ATLAS_BYTES {
        return DisunityResult::invalid_argument();
    }
    let r = unsafe { *render };
    let mode = match r.mode {
        0 => RenderMode::Coverage,
        1 => RenderMode::Sdf,
        _ => return DisunityResult::invalid_argument(),
    };
    let job = GlyphJob {
        glyph: r.glyph,
        mode,
        padding: r.padding,
        rect: GlyphRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        },
    };
    let pixels = unsafe { std::slice::from_raw_parts_mut(atlas, atlas_length as usize) };
    mutate(world, |s| {
        s.faces
            .render_glyph(face_of(face)?, job, pixels, r.atlas_width, r.atlas_height)
            .map_err(AbiError::from)
    })
}

/// Writes the kerning of every ordered pair of `glyphs` that the face adjusts. `count` holds the
/// record capacity on entry and the number of adjustments on return. A zero capacity asks for the
/// count and copies nothing; with more adjustments than a nonzero capacity nothing is copied and
/// the call fails.
///
/// # Safety
/// `glyphs` is readable for `glyph_count` values; `records` is writable for the capacity in
/// `count`, or null when it is zero; `count` is readable and writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_face_pairs(
    world: u64,
    face: u64,
    glyphs: *const u32,
    glyph_count: u32,
    records: *mut DisunityGlyphPair,
    count: *mut u32,
) -> DisunityResult {
    if count.is_null() || (glyph_count > 0 && glyphs.is_null()) {
        return DisunityResult::invalid_argument();
    }
    let capacity = unsafe { *count };
    if capacity > 0 && records.is_null() {
        return DisunityResult::invalid_argument();
    }
    let glyphs = if glyph_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(glyphs, glyph_count as usize) }
    };
    mutate(world, |s| {
        let pairs = s
            .faces
            .pair_adjustments(face_of(face)?, glyphs)
            .map_err(AbiError::from)?;
        let needed = u32::try_from(pairs.len()).map_err(|_| "too many glyph pairs")?;
        unsafe { *count = needed };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < needed {
            return Err("glyph pair capacity is too small".into());
        }
        for (i, p) in pairs.iter().enumerate() {
            unsafe {
                *records.add(i) = DisunityGlyphPair {
                    first: p.first,
                    second: p.second,
                    x_advance: p.x_advance,
                };
            }
        }
        Ok(())
    })
}
