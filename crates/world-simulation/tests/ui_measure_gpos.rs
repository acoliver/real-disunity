//! Atlas-free measurement uses the same synthetic GPOS fixtures as glyph placement.
#![cfg(feature = "ui")]
#[path = "ui_font/positioning.rs"]
mod positioning;
mod ui_font;

use glam::Vec2;
use positioning::{Lookup, default_gpos, font, kern};
use worldrunner_world_simulation::ui::{FontBook, TextRun, UiLimits};

#[test]
fn pair_formats_extensions_and_precedence_share_measured_advances() {
    for format in [1, 2] {
        for extension in [false, true] {
            let bytes = font(
                Some(default_gpos(&[Lookup::pair(format, extension)])),
                Some(kern(&[(1, 2, -50)])),
            );
            let mut fonts = FontBook::new(UiLimits::default()).unwrap();
            let id = fonts.register(bytes).unwrap();
            let run = TextRun {
                text: "AV".into(),
                font_size: 20.,
                ..TextRun::new(id)
            };
            for (width, expected) in [(100., vec![22.]), (23., vec![22.]), (21., vec![12., 12.])] {
                let size = Vec2::new(width, 100.);
                let measurement = fonts.measure(&run, size).unwrap();
                assert_eq!(
                    measurement
                        .lines
                        .iter()
                        .map(|l| l.width)
                        .collect::<Vec<_>>(),
                    expected
                );
                assert_eq!(fonts.cache_stats().atlas_builds, 0);
            }
            let m = fonts.measure(&run, Vec2::new(100., 100.)).unwrap();
            let layout = fonts.layout(&run, Vec2::new(100., 100.)).unwrap();
            assert_eq!(layout.lines, m.lines);
            assert_eq!(layout.glyphs[1].origin.x.to_bits(), 10_f32.to_bits());
        }
    }
}

#[test]
fn unadjusted_measurement_uses_ordinary_advances() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let run = TextRun {
        text: "AV".into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    assert_eq!(
        fonts
            .measure(&run, Vec2::splat(100.))
            .unwrap()
            .width
            .to_bits(),
        24_f32.to_bits()
    );
}
