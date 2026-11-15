//! UTF-8 boundary protocols and atlas-free prefix fitting use synthetic fonts only.
#![cfg(feature = "ui")]
#[path = "ui_font/positioning.rs"]
mod positioning;
mod ui_font;

use glam::Vec2;
use worldrunner_world_simulation::ui::{
    FontBook, HorizontalOverflow, LineBreakCandidates, TextRun, UiError, UiLimits, advance_text,
    line_break_info,
};

#[test]
fn candidates_return_break_character_bytes_then_one_terminal_boundary() {
    let breaks = LineBreakCandidates::new(vec![(0x3000, 0x30ff), (0x4e00, 0x9fff)]).unwrap();
    let text = "AB C EF在茂。密";
    for (start, expected) in [(0, 2), (2, 4), (4, 7), (7, 10), (10, 16), (16, 19)] {
        assert_eq!(breaks.next(text, start, "。", "").unwrap(), Some(expected));
    }
    assert_eq!(breaks.next(text, 19, "。", "").unwrap(), None);
    assert_eq!(breaks.next("word", 0, "", "").unwrap(), Some(4));
    assert_eq!(breaks.next("", 0, "", "").unwrap(), None);
    assert_eq!(breaks.next("é", 1, "", ""), Err(UiError::InvalidText));
    assert_eq!(breaks.next("a", 2, "", ""), Err(UiError::InvalidText));
}

#[test]
fn caller_ranges_and_forbidden_sets_control_boundaries() {
    let breaks = LineBreakCandidates::new(vec![(0x4e00, 0x9fff)]).unwrap();
    assert_eq!(breaks.next("一二三", 0, "二", ""), Ok(Some(6)));
    assert_eq!(breaks.next("一二三", 0, "", "一"), Ok(Some(6)));
    assert_eq!(breaks.next("a b", 0, "", ""), Ok(Some(1)));
    assert_eq!(breaks.next("a-b/c", 0, "", ""), Ok(Some(5)));
    assert!(LineBreakCandidates::new(vec![(3, 2)]).is_err());
    assert!(LineBreakCandidates::new(vec![(0, 0x0011_0000)]).is_err());
    assert_eq!(line_break_info("»word(", "»", "("), (true, true));
    assert_eq!(line_break_info("»word(", "!", ")"), (false, false));
    assert_eq!(line_break_info("", "", ""), (false, false));
}

#[test]
fn advance_uses_checked_bytes_and_clamps_character_counts() {
    assert_eq!(advance_text("éAB", 0, 1), Ok(2));
    assert_eq!(advance_text("éAB", 2, -1), Ok(0));
    assert_eq!(advance_text("éAB", 3, i64::MAX), Ok(4));
    assert_eq!(advance_text("éAB", 4, i64::MIN), Ok(0));
    assert_eq!(advance_text("éAB", 1, 1), Err(UiError::InvalidText));
}

fn fixture() -> (FontBook, TextRun) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mapping: Vec<_> = (32..=255).map(|c| (c, 1)).collect();
    let bytes = ui_font::assemble_with(
        &[Vec::new(), ui_font::glyph(1)],
        800,
        -200,
        vec![(*b"cmap", ui_font::cmap_with(&mapping))],
    );
    let id = fonts.register(bytes).unwrap();
    let run = TextRun {
        font_size: 20.,
        rich_text: false,
        horizontal_overflow: HorizontalOverflow::Overflow,
        ..TextRun::new(id)
    };
    (fonts, run)
}

#[test]
fn fitting_is_utf8_safe_and_agrees_with_measurement_without_an_atlas() {
    let (fonts, mut run) = fixture();
    for text in ["", "ABéC", "A A", "A\rB", "A\nBB", "\n"] {
        run.text = text.into();
        for width in [0., 12., 24., 25., 48.] {
            let expected = (0..=text.len())
                .filter(|&end| text.is_char_boundary(end))
                .rfind(|&end| {
                    let prefix = run.substring(0..end).unwrap();
                    fonts.measure(&prefix, Vec2::ZERO).unwrap().width <= width
                })
                .unwrap();
            assert_eq!(fonts.fit_prefix(&run, width).unwrap(), expected, "{text:?}");
        }
    }
    assert_eq!(fonts.cache_stats().atlas_builds, 0);
    assert_eq!(fonts.cache_stats().layouts, 0);
    assert_eq!(fonts.fit_prefix(&run, f32::NAN), Err(UiError::InvalidRect));
    assert_eq!(fonts.fit_prefix(&run, -1.), Err(UiError::InvalidRect));
}

#[test]
fn fitting_shares_gpos_pair_advances_and_resets_at_newlines() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts
        .register(positioning::font(
            Some(positioning::default_gpos(&[positioning::Lookup::pair(
                1, false,
            )])),
            Some(positioning::kern(&[(1, 2, -50)])),
        ))
        .unwrap();
    let mut run = TextRun {
        text: "AV".into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    assert_eq!(fonts.fit_prefix(&run, 22.), Ok(2));
    assert_eq!(fonts.fit_prefix(&run, 21.), Ok(1));
    run.text = "A\nV".into();
    assert_eq!(fonts.fit_prefix(&run, 12.), Ok(3));
    run.font = fonts.register(ui_font::font()).unwrap();
    run.text = "é".into();
    assert_eq!(fonts.fit_prefix(&run, 0.), Err(UiError::MissingGlyph('é')));
}

#[test]
fn fitting_handles_a_full_budget_text_run_in_one_metric_pass() {
    let (fonts, mut run) = fixture();
    run.text = "A".repeat(4096);
    assert_eq!(fonts.fit_prefix(&run, 24.), Ok(2));
}

#[test]
fn fitting_scans_past_a_wide_prefix_when_a_later_pair_reduces_its_advance() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts
        .register(positioning::font(
            None,
            Some(positioning::kern(&[(1, 2, -700)])),
        ))
        .unwrap();
    let run = TextRun {
        text: "AV".into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    assert_eq!(fonts.fit_prefix(&run, 11.), Ok(2));
    assert_eq!(
        fonts
            .measure(&run, Vec2::splat(100.))
            .unwrap()
            .width
            .to_bits(),
        10_f32.to_bits()
    );
}
