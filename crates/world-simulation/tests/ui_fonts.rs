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

fn single_point() -> Vec<u8> {
    let mut glyph = vec![0; 10];
    glyph[..2].copy_from_slice(&1_i16.to_be_bytes());
    glyph.extend(0_u16.to_be_bytes());
    glyph.extend(7_u16.to_be_bytes());
    glyph.extend([0; 7]);
    glyph.extend([0x31, 0, 0]); // On-curve, unchanged X/Y, followed by padding.
    glyph
}

fn collinear() -> Vec<u8> {
    let mut glyph = ui_font::glyph(1);
    glyph[8..10].copy_from_slice(&500_i16.to_be_bytes());
    for (index, delta) in [0_i16, 500, -500].into_iter().enumerate() {
        glyph[23 + index * 2..25 + index * 2].copy_from_slice(&delta.to_be_bytes());
    }
    glyph
}

fn text_layout(
    fonts: &mut FontBook,
    bytes: Vec<u8>,
    text: &str,
) -> std::sync::Arc<worldrunner_world_simulation::ui::TextLayout> {
    let id = fonts.register(bytes).unwrap();
    fonts
        .layout(
            &TextRun {
                text: text.into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.),
        )
        .unwrap()
}

#[test]
fn unmapped_single_point_preserves_text_measurement_and_layout() {
    let mut fonts = book();
    let expected = text_layout(&mut fonts, ui_font::font(), "AA A\nAA");
    let actual = text_layout(&mut fonts, font_with_glyph(single_point()), "AA A\nAA");
    assert_eq!(actual, expected);
}

fn assert_empty_with_horizontal_metrics(glyph: Vec<u8>) {
    let mut fonts = book();
    let expected = text_layout(
        &mut fonts,
        ui_font::assemble(&[Vec::new(), Vec::new()], 800, -200),
        "AA",
    );
    assert_eq!(
        expected.lines[0].width.to_bits(),
        (2_f32 * (14. / 1000. * 600.)).to_bits()
    );
    let actual = text_layout(
        &mut fonts,
        ui_font::assemble(&[Vec::new(), glyph], 800, -200),
        "AA",
    );
    assert_eq!(actual, expected);
    assert!(actual.glyphs.is_empty());
    assert!(actual.atlas.alpha.is_empty());
}

#[test]
fn mapped_single_point_is_empty_but_keeps_horizontal_metrics() {
    assert_empty_with_horizontal_metrics(single_point());
}

#[test]
fn collinear_contour_is_empty_but_keeps_horizontal_metrics() {
    assert_empty_with_horizontal_metrics(collinear());
}

#[test]
fn repeated_short_coordinate_flags_preserve_empty_contours() {
    let mut glyph = vec![0; 10];
    glyph[..2].copy_from_slice(&1_i16.to_be_bytes());
    glyph.extend(2_u16.to_be_bytes());
    glyph.extend(0_u16.to_be_bytes());
    // Two positive short X/Y deltas, then a negative short delta on both axes.
    glyph.extend([0x3f, 1, 0x07, 100, 50, 50, 100, 50, 50, 0]);
    assert_empty_with_horizontal_metrics(glyph);
}

#[test]
fn degenerate_contours_are_checked_separately_and_do_not_hide_drawable_contours() {
    let mut glyph = ui_font::glyph(2);
    for (index, delta) in [0_i16, 500, -500, 0, 0, 0].into_iter().enumerate() {
        glyph[34 + index * 2..36 + index * 2].copy_from_slice(&delta.to_be_bytes());
    }
    assert_empty_with_horizontal_metrics(glyph.clone());
    glyph[44..46].copy_from_slice(&700_i16.to_be_bytes());
    let actual = text_layout(
        &mut book(),
        ui_font::assemble(&[Vec::new(), glyph], 800, -200),
        "A",
    );
    assert_eq!(actual.glyphs.len(), 1);
    assert!(actual.atlas.alpha.contains(&255));
}

#[test]
fn empty_glyph_index_obeys_font_storage_limit() {
    let bytes = font_with_glyph(single_point());
    let mut fonts = FontBook::new(UiLimits {
        max_font_bytes: bytes.len() + 1,
        ..UiLimits::default()
    })
    .unwrap();
    assert_eq!(fonts.register(bytes.clone()), Err(UiError::FontLimit));
    let mut fonts = FontBook::new(UiLimits {
        max_font_bytes: bytes.len() + size_of::<u16>(),
        ..UiLimits::default()
    })
    .unwrap();
    assert!(fonts.register(bytes).is_ok());
}

#[test]
fn malformed_unmapped_simple_glyphs_are_invalid_fonts() {
    let mut flags_cut_short = single_point();
    flags_cut_short.truncate(14);
    flags_cut_short[12..14].fill(0);
    let mut coordinates_cut_short = collinear();
    coordinates_cut_short.truncate(28);
    let mut endpoint_past_points = single_point();
    endpoint_past_points[10..12].copy_from_slice(&1000_u16.to_be_bytes());
    let mut instructions_past_end = single_point();
    instructions_past_end[12..14].copy_from_slice(&1000_u16.to_be_bytes());
    let mut endpoint_overflow = single_point();
    endpoint_overflow[10..12].copy_from_slice(&u16::MAX.to_be_bytes());
    let mut repeated_endpoint = ui_font::glyph(2);
    repeated_endpoint[12..14].copy_from_slice(&2_u16.to_be_bytes());
    let mut descending_endpoint = ui_font::glyph(2);
    descending_endpoint[12..14].copy_from_slice(&1_u16.to_be_bytes());
    let mut repeat_past_points = single_point();
    repeat_past_points[21..24].copy_from_slice(&[0x39, 1, 0]);
    let mut repeat_cut_short = single_point();
    repeat_cut_short[12..14].fill(0);
    repeat_cut_short[14..16].copy_from_slice(&[0x39, 0]);
    repeat_cut_short.truncate(16);
    repeat_cut_short[10..12].copy_from_slice(&1_u16.to_be_bytes());
    let mut missing_repeat_count = repeat_cut_short.clone();
    missing_repeat_count[14..16].copy_from_slice(&[0x31, 0x39]);
    for (glyph, case) in [
        (flags_cut_short, "missing flags"),
        (coordinates_cut_short, "truncated coordinates"),
        (endpoint_past_points, "endpoint beyond available points"),
        (instructions_past_end, "instructions past the glyph record"),
        (endpoint_overflow, "endpoint overflows the point count"),
        (missing_repeat_count, "repeat flag without its count byte"),
        (repeated_endpoint, "repeated contour endpoint"),
        (descending_endpoint, "descending contour endpoints"),
        (repeat_past_points, "flag repetition exceeds point count"),
        (
            repeat_cut_short,
            "flags end before the declared point count",
        ),
    ] {
        assert_eq!(
            book().register(font_with_glyph(glyph)),
            Err(UiError::InvalidFont),
            "{case}"
        );
    }
}
