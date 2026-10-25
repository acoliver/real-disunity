use super::UiError;
use ab_glyph::{Font, FontRef, GlyphId};
use ttf_parser::{Face, Tag, loca};

pub(super) fn validate(bytes: &[u8]) -> Result<(), UiError> {
    let face = Face::parse(bytes, 0).map_err(|_| UiError::InvalidFont)?;
    let font = FontRef::try_from_slice(bytes).map_err(|_| UiError::InvalidFont)?;
    if !font.height_unscaled().is_finite() || font.height_unscaled() <= 0. {
        return Err(UiError::InvalidFont);
    }
    let tables = face.tables();
    let metrics = tables.hmtx.ok_or(UiError::InvalidFont)?;
    let raw = face.raw_face();
    let glyf = raw
        .table(Tag::from_bytes(b"glyf"))
        .ok_or(UiError::InvalidFont)?;
    let locations = loca::Table::parse(
        tables.maxp.number_of_glyphs,
        tables.head.index_to_location_format,
        raw.table(Tag::from_bytes(b"loca"))
            .ok_or(UiError::InvalidFont)?,
    )
    .ok_or(UiError::InvalidFont)?;
    if tables.glyf.is_none()
        || tables.cmap.is_none()
        || (b' '..=b'~').any(|c| font.glyph_id(char::from(c)).0 >= face.number_of_glyphs())
        || usize::from(locations.len()) != usize::from(face.number_of_glyphs()) + 1
    {
        return Err(UiError::InvalidFont);
    }
    for id in 0..face.number_of_glyphs() {
        let glyph = ttf_parser::GlyphId(id);
        if metrics.advance(glyph).is_none() || metrics.side_bearing(glyph).is_none() {
            return Err(UiError::InvalidFont);
        }
        let start = offset(locations, id).ok_or(UiError::InvalidFont)?;
        let end = offset(locations, id + 1).ok_or(UiError::InvalidFont)?;
        let data = glyf.get(start..end).ok_or(UiError::InvalidFont)?;
        // Equal offsets and zero-contour records are authored empty glyphs.
        if data.is_empty() {
            continue;
        }
        let header = data.get(..10).ok_or(UiError::InvalidFont)?;
        if header[..2] != [0, 0] && font.outline(GlyphId(id)).is_none() {
            return Err(UiError::InvalidFont);
        }
    }
    Ok(())
}

fn offset(locations: loca::Table<'_>, index: u16) -> Option<usize> {
    match locations {
        loca::Table::Short(values) => Some(usize::from(values.get(index)?) * 2),
        loca::Table::Long(values) => usize::try_from(values.get(index)?).ok(),
    }
}
