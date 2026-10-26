#![cfg(feature = "ui")]

mod ui_font;
use glam::Vec2;
use worldrunner_world_simulation::ui::{FontBook, TextRun, UiError, UiLimits};

fn table_record(bytes: &[u8], tag: [u8; 4]) -> usize {
    (12..12 + usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap())) * 16)
        .step_by(16)
        .find(|i| bytes[*i..*i + 4] == tag)
        .unwrap()
}
fn table_offset(bytes: &[u8], tag: [u8; 4]) -> usize {
    let record = table_record(bytes, tag);
    usize::try_from(u32::from_be_bytes(
        bytes[record + 8..record + 12].try_into().unwrap(),
    ))
    .unwrap()
}
fn book() -> FontBook {
    FontBook::new(UiLimits::default()).unwrap()
}

#[test]
fn overlapping_contours_saturate_alpha_without_panicking() {
    let mut fonts = book();
    let id = fonts.register(ui_font::font_with(2, 800, -200)).unwrap();
    let text = fonts
        .layout(
            &TextRun {
                text: "AA".into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.),
        )
        .unwrap();
    assert_eq!(text.glyphs.len(), 2);
    assert!(text.atlas.alpha.contains(&255));
}

#[test]
fn missing_or_truncated_required_tables_are_invalid_fonts() {
    for tag in [*b"hmtx", *b"loca", *b"glyf"] {
        let mut bytes = ui_font::font();
        let record = table_record(&bytes, tag);
        bytes[record + 12..record + 16].copy_from_slice(&0_u32.to_be_bytes());
        assert_eq!(book().register(bytes), Err(UiError::InvalidFont), "{tag:?}");
    }
}

#[test]
fn unsupported_outline_representation_is_a_typed_error() {
    let mut bytes = ui_font::font();
    let record = table_record(&bytes, *b"glyf");
    bytes[record..record + 4].copy_from_slice(b"SVG ");
    assert_eq!(book().register(bytes), Err(UiError::InvalidFont));
}

#[test]
fn truncated_bearings_and_out_of_range_mappings_are_invalid() {
    let mut bytes = ui_font::font();
    let hhea = table_offset(&bytes, *b"hhea");
    bytes[hhea + 34..hhea + 36].copy_from_slice(&1_u16.to_be_bytes());
    let record = table_record(&bytes, *b"hmtx");
    bytes[record + 12..record + 16].copy_from_slice(&4_u32.to_be_bytes());
    assert_eq!(book().register(bytes), Err(UiError::InvalidFont));

    let mut bytes = ui_font::font();
    let cmap = table_offset(&bytes, *b"cmap");
    // Format 4 idDelta for the first ASCII character maps to glyph 2, past maxp.
    let delta = cmap + 12 + 14 + 96 * 4 + 2;
    bytes[delta..delta + 2].copy_from_slice(&2_u16.wrapping_sub(32).to_be_bytes());
    assert_eq!(book().register(bytes), Err(UiError::InvalidFont));
}

#[test]
fn corrupt_nonempty_outline_is_not_successful_empty_geometry() {
    let mut bytes = ui_font::font();
    let offset = table_offset(&bytes, *b"glyf");
    bytes[offset + 10..offset + 12].copy_from_slice(&1000_u16.to_be_bytes());
    assert_eq!(book().register(bytes), Err(UiError::InvalidFont));
}

#[test]
fn zero_height_metrics_are_rejected_before_publication() {
    let mut bytes = ui_font::font();
    let offset = table_offset(&bytes, *b"hhea");
    bytes[offset + 4..offset + 8].fill(0);
    assert_eq!(book().register(bytes), Err(UiError::InvalidFont));
}

#[test]
fn zero_advance_and_intentionally_empty_glyphs_remain_valid() {
    let mut bytes = ui_font::font();
    let offset = table_offset(&bytes, *b"hmtx");
    bytes[offset + 4..offset + 6].fill(0);
    let mut fonts = book();
    let id = fonts.register(bytes).unwrap();
    let text = fonts
        .layout(
            &TextRun {
                text: "AA".into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.),
        )
        .unwrap();
    assert_eq!(text.lines[0].width.to_bits(), 0_f32.to_bits());
    assert_eq!(text.glyphs.len(), 2);
    let mut bytes = ui_font::font();
    let offset = table_offset(&bytes, *b"loca");
    bytes[offset + 4..offset + 6].fill(0);
    let id = fonts.register(bytes).unwrap();
    let text = fonts
        .layout(
            &TextRun {
                text: " ".into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.),
        )
        .unwrap();
    assert!(text.lines[0].width > 0.);
    assert!(text.glyphs.is_empty());
}

/// The default font with `extra` appended as glyph 2, which no character maps to.
fn font_with_glyph(extra: Vec<u8>) -> Vec<u8> {
    ui_font::assemble(&[Vec::new(), ui_font::glyph(1), extra], 800, -200)
}

/// A composite glyph: one record per `(glyph, more)` component, offset by word arguments.
fn composite(components: &[(u16, bool)]) -> Vec<u8> {
    let mut glyf = Vec::new();
    for n in [-1_i16, 0, 0, 0, 0] {
        glyf.extend(n.to_be_bytes());
    }
    for &(component, more) in components {
        let flags = 0x0003_u16 | if more { 0x0020 } else { 0 };
        for n in [flags, component, 0, 0] {
            glyf.extend(n.to_be_bytes());
        }
    }
    glyf
}

#[test]
fn composites_of_empty_glyphs_are_empty_glyphs() {
    // Fonts such as Liberation Sans draw no-break spaces as a composite of the space glyph.
    let empty = composite(&[(0, false)]);
    assert!(book().register(font_with_glyph(empty)).is_ok());
    let drawn = composite(&[(1, false)]);
    assert!(book().register(font_with_glyph(drawn)).is_ok());
    for (components, case) in [
        (vec![(9, false)], "a component past the glyph count"),
        (vec![(2, false)], "a composite of itself"),
        (
            vec![(0, true)],
            "records that end before their last component",
        ),
    ] {
        let bytes = font_with_glyph(composite(&components));
        assert_eq!(book().register(bytes), Err(UiError::InvalidFont), "{case}");
    }
}
