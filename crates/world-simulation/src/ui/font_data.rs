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
        let data = glyph_data(glyf, locations, id).ok_or(UiError::InvalidFont)?;
        // Equal offsets are an authored empty glyph.
        if !data.is_empty() && !outline_valid(&font, data, glyf, locations, id) {
            return Err(UiError::InvalidFont);
        }
    }
    Ok(())
}

/// Whether nonempty glyph `id` has an outline, or is legitimately empty: zero contours, or a
/// composite whose well-formed components are all empty glyphs.
fn outline_valid(
    font: &FontRef<'_>,
    data: &[u8],
    glyf: &[u8],
    locations: loca::Table<'_>,
    id: u16,
) -> bool {
    let Some(&[high, low]) = data.get(..2) else {
        return false;
    };
    let contours = i16::from_be_bytes([high, low]);
    if contours == 0 || font.outline(GlyphId(id)).is_some() {
        return data.len() >= 10;
    }
    if contours > 0 {
        return false;
    }
    components(data).is_some_and(|ids| {
        ids.iter().all(|&component| {
            component != id
                && font.outline(GlyphId(component)).is_none()
                && glyph_data(glyf, locations, component).is_some()
        })
    })
}

fn glyph_data<'a>(glyf: &'a [u8], locations: loca::Table<'_>, id: u16) -> Option<&'a [u8]> {
    glyf.get(offset(locations, id)?..offset(locations, id + 1)?)
}

/// The component glyph IDs of a composite glyph, or `None` when its records are truncated.
fn components(data: &[u8]) -> Option<Vec<u16>> {
    const WORD_ARGUMENTS: u16 = 0x0001;
    const SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const XY_SCALE: u16 = 0x0040;
    const TWO_BY_TWO: u16 = 0x0080;
    let word = |at: usize| {
        data.get(at..at + 2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
    };
    let mut at = 10;
    let mut ids = Vec::new();
    loop {
        let flags = word(at)?;
        ids.push(word(at + 2)?);
        let arguments = if flags & WORD_ARGUMENTS == 0 { 2 } else { 4 };
        let transform = match (flags & TWO_BY_TWO, flags & XY_SCALE, flags & SCALE) {
            (0, 0, 0) => 0,
            (0, 0, _) => 2,
            (0, _, _) => 4,
            _ => 8,
        };
        at += 4 + arguments + transform;
        data.get(..at)?;
        if flags & MORE_COMPONENTS == 0 {
            return Some(ids);
        }
    }
}

fn offset(locations: loca::Table<'_>, index: u16) -> Option<usize> {
    match locations {
        loca::Table::Short(values) => Some(usize::from(values.get(index)?) * 2),
        loca::Table::Long(values) => usize::try_from(values.get(index)?).ok(),
    }
}
