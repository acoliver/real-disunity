//! Shared metrics, UTF-8 provenance and coverage layout, using synthetic outlines only.
#![cfg(feature = "ui")]
mod ui_font;

use glam::Vec2;
use std::sync::Arc;
use worldrunner_world_simulation::ui::{
    FontBook, GlyphRaster, HorizontalAlign, HorizontalOverflow, RenderMode, TextRun, TextSpan,
    UiError, UiLimits, VerticalAlign, VerticalOverflow,
};

fn fixture(text: &str) -> (FontBook, TextRun) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mapping: Vec<_> = (32..=255)
        .map(|c| (c, if c == 32 { 2 } else { 1 }))
        .collect();
    let bytes = ui_font::assemble_with(
        &[Vec::new(), ui_font::glyph(1), Vec::new()],
        1200,
        -300,
        vec![(*b"cmap", ui_font::cmap_with(&mapping))],
    );
    let id = fonts.register(bytes).unwrap();
    (
        fonts,
        TextRun {
            text: text.into(),
            font_size: 20.,
            ..TextRun::new(id)
        },
    )
}

#[test]
fn distance_raster_requires_positive_spread() {
    let (fonts, run) = fixture("A");
    assert_eq!(
        fonts.layout_with_raster(
            &run,
            Vec2::splat(100.),
            GlyphRaster {
                mode: RenderMode::Sdf,
                padding: 0,
            },
        ),
        Err(UiError::DistanceFieldLimit)
    );
}

#[test]
fn multibyte_word_wraps_at_adjacent_character_byte_boundaries() {
    let (fonts, run) = fixture("ééé");
    let size = Vec2::new(18., 200.);
    let measured = fonts.measure(&run, size).unwrap();
    let layout = fonts.layout(&run, size).unwrap();
    assert_eq!(measured.lines, layout.lines);
    assert_eq!(
        measured
            .lines
            .iter()
            .map(|l| (l.source_range.clone(), l.next_byte))
            .collect::<Vec<_>>(),
        [(0..2, 2), (2..4, 4), (4..6, 6)]
    );
    for line in &measured.lines {
        assert_eq!(line.source(&run.text).unwrap(), "é");
    }
}

#[test]
fn advance_width_is_not_ink_and_measurement_builds_no_atlas() {
    let (fonts, run) = fixture("A A ");
    let measured = fonts.measure(&run, Vec2::new(100., 100.)).unwrap();
    assert_eq!((measured.width, measured.height), (36., 30.));
    assert_eq!(fonts.cache_stats().atlas_builds, 0);
    assert_eq!(fonts.cache_stats().layouts, 0);
    let layout = fonts.layout(&run, Vec2::new(100., 100.)).unwrap();
    assert_eq!(measured.lines, layout.lines);
    assert_eq!(layout.glyphs.len(), 2, "empty outlines still advance");
    assert_eq!(f32::to_bits(layout.glyphs[1].origin.x), f32::to_bits(18.));
    assert_eq!(
        f32::to_bits(layout.glyphs[0].rect_max.x - layout.glyphs[0].rect_min.x),
        f32::to_bits(10.)
    );
}

#[test]
fn full_blank_trailing_and_empty_heights_include_paragraph_spacing() {
    for (text, count, height) in [
        ("", 0, 0.),
        ("\n", 2, 65.),
        ("A\n\n", 3, 100.),
        (" ", 1, 30.),
    ] {
        let (fonts, mut run) = fixture(text);
        run.paragraph_spacing = 5.;
        let m = fonts.measure(&run, Vec2::splat(100.)).unwrap();
        assert_eq!(m.lines.len(), count, "{text:?}");
        assert_eq!(f32::to_bits(m.height), f32::to_bits(height), "{text:?}");
    }
    let (fonts, mut run) = fixture("A\nA\nA");
    run.line_height = Some(40.);
    run.paragraph_spacing = 5.;
    let m = fonts.measure(&run, Vec2::new(100., 1.)).unwrap();
    assert_eq!(
        f32::to_bits(m.height),
        f32::to_bits(120.),
        "measurement ignores vertical truncation"
    );
    assert_eq!(m.lines.len(), 3);
}

#[test]
fn wrap_ranges_consume_spaces_and_preserve_multibyte_crlf_positions() {
    let (fonts, run) = fixture("éé AA\r\n\nA\n");
    let m = fonts.measure(&run, Vec2::new(30., 200.)).unwrap();
    let ranges: Vec<_> = m
        .lines
        .iter()
        .map(|l| (l.source_range.clone(), l.next_byte))
        .collect();
    assert_eq!(
        ranges,
        [(0..4, 5), (5..8, 9), (9..9, 10), (10..11, 12), (12..12, 12)]
    );
    assert_eq!(m.lines[1].source(&run.text).unwrap(), "AA\r");
    assert_eq!(f32::to_bits(m.lines[0].width), f32::to_bits(24.));
    let (fonts, run) = fixture("AA AA");
    let m = fonts.measure(&run, Vec2::new(30., 100.)).unwrap();
    assert_eq!(
        m.lines.iter().map(|l| l.width).collect::<Vec<_>>(),
        [24., 24.]
    );
}

#[test]
fn substring_validates_utf8_and_rebases_intersecting_spans() {
    let (_, mut run) = fixture("AéB");
    run.spans.push(TextSpan {
        range: 1..4,
        color: [0.5; 4],
        link: None,
    });
    let part = run.substring(1..3).unwrap();
    assert_eq!(part.text, "é");
    assert_eq!(part.spans[0].range, 0..2);
    assert_eq!(run.substring(2..3), Err(UiError::InvalidText));
    assert_eq!(run.substring(0..99), Err(UiError::InvalidText));
}

#[test]
fn alignment_and_overflow_use_the_same_line_stage() {
    let (fonts, mut run) = fixture("AA\nA");
    run.horizontal = HorizontalAlign::Right;
    run.vertical = VerticalAlign::Middle;
    run.vertical_overflow = VerticalOverflow::Overflow;
    run.horizontal_overflow = HorizontalOverflow::Overflow;
    let size = Vec2::new(100., 120.);
    let m = fonts.measure(&run, size).unwrap();
    let l = fonts.layout(&run, size).unwrap();
    assert_eq!(m.lines, l.lines);
    assert_eq!((m.width, m.height), (24., 60.));
    assert_eq!(l.glyphs[0].origin, Vec2::new(76., 66.));
    assert_eq!(l.glyphs[2].origin, Vec2::new(88., 36.));
}

#[test]
fn measure_checks_glyph_font_and_shaping_without_raster_budget() {
    let (fonts, run) = fixture("A");
    let empty = FontBook::new(UiLimits::default()).unwrap();
    assert_eq!(
        empty.measure(&run, Vec2::splat(100.)),
        Err(UiError::MissingFont(run.font))
    );
    let mut bad = run.clone();
    bad.text = "中".into();
    assert_eq!(
        fonts.measure(&bad, Vec2::splat(100.)),
        Err(UiError::UnsupportedShaping('中'))
    );
    let mut small = FontBook::new(UiLimits {
        max_atlas_pixels: 1,
        ..UiLimits::default()
    })
    .unwrap();
    let id = small.register(ui_font::font()).unwrap();
    let mut text = TextRun {
        text: "A".into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    assert_eq!(
        f32::to_bits(small.measure(&text, Vec2::splat(100.)).unwrap().width),
        f32::to_bits(12.)
    );
    assert_eq!(
        small.layout(&text, Vec2::splat(100.)),
        Err(UiError::AtlasLimit)
    );
    text.text = "é".into();
    assert_eq!(
        small.measure(&text, Vec2::splat(100.)),
        Err(UiError::MissingGlyph('é'))
    );
}

#[test]
fn coverage_entry_point_preserves_bytes_and_keys_padding() {
    let (fonts, run) = fixture("A");
    let size = Vec2::splat(100.);
    let ordinary = fonts.layout(&run, size).unwrap();
    let same = fonts
        .layout_with_raster(&run, size, GlyphRaster::default())
        .unwrap();
    assert!(Arc::ptr_eq(&ordinary, &same));
    let padded = fonts
        .layout_with_raster(
            &run,
            size,
            GlyphRaster {
                mode: RenderMode::Coverage,
                padding: 2,
            },
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&ordinary, &padded));
    assert_eq!(padded.lines, ordinary.lines);
    assert_eq!(
        padded.glyphs[0].rect_min,
        ordinary.glyphs[0].rect_min - Vec2::splat(2.)
    );
    let a = &ordinary.glyphs[0];
    let b = &padded.glyphs[0];
    let stride = usize::try_from(ordinary.atlas.size.x).unwrap();
    let padded_stride = usize::try_from(padded.atlas.size.x).unwrap();
    for y in 0..a.atlas_size.y {
        for x in 0..a.atlas_size.x {
            let old = usize::try_from(a.atlas_min.y + y).unwrap() * stride
                + usize::try_from(a.atlas_min.x + x).unwrap();
            let new = usize::try_from(b.atlas_min.y + y + 2).unwrap() * padded_stride
                + usize::try_from(b.atlas_min.x + x + 2).unwrap();
            assert_eq!(ordinary.atlas.alpha[old], padded.atlas.alpha[new]);
        }
    }
}
