//! Text layout follows Unity's `Text`: font size is pixels per em, lines wrap at word boundaries,
//! and truncation drops the lines that do not fit.
#![cfg(feature = "ui")]
mod ui_font;

use glam::Vec2;
use worldrunner_world_simulation::ui::{
    FontBook, HorizontalOverflow, TextLayout, TextRun, UiLimits, VerticalOverflow,
};

/// Every printable character in the test font is one glyph 600 units wide, 12 px at size 20.
const ADVANCE: f32 = 12.;

fn layout(text: &str, size: Vec2, change: impl FnOnce(&mut TextRun)) -> TextLayout {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let mut run = TextRun {
        text: text.into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    change(&mut run);
    fonts.layout(&run, size).unwrap()
}

fn widths(layout: &TextLayout) -> Vec<f32> {
    layout.lines.iter().map(|line| line.width).collect()
}

#[test]
fn font_size_is_pixels_per_em_whatever_the_line_height() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font_with(1, 1200, -300)).unwrap();
    let run = TextRun {
        text: "A".into(),
        font_size: 20.,
        horizontal_overflow: HorizontalOverflow::Overflow,
        ..TextRun::new(id)
    };
    let layout = fonts.layout(&run, Vec2::splat(100.)).unwrap();
    assert_eq!(
        widths(&layout),
        [ADVANCE],
        "600 units of a 1000-unit em at 20 px"
    );
    let line_height = 30.;
    let short = fonts
        .layout(&run, Vec2::new(100., line_height - 1.))
        .unwrap();
    assert!(short.lines.is_empty(), "a 1.5 em line does not fit 29 px");
    let exact = fonts.layout(&run, Vec2::new(100., line_height)).unwrap();
    assert_eq!(exact.lines.len(), 1);
}

#[test]
fn wrapping_breaks_before_the_last_word_and_consumes_the_space() {
    let narrow = Vec2::new(5. * ADVANCE - 10., 100.);
    let words = layout("AA AA", narrow, |_| {});
    assert_eq!(widths(&words), [2. * ADVANCE, 2. * ADVANCE]);
    assert_eq!(words.glyphs.len(), 4, "the space at the break is not drawn");
    let trailing = layout("AAAA AA", narrow, |_| {});
    assert_eq!(widths(&trailing), [4. * ADVANCE, 2. * ADVANCE]);
    let long = layout("AAAAA", narrow, |_| {});
    assert_eq!(
        widths(&long),
        [4. * ADVANCE, ADVANCE],
        "long words break by character"
    );
    let kept = layout("AA AA", narrow, |run| {
        run.horizontal_overflow = HorizontalOverflow::Overflow;
    });
    assert_eq!(widths(&kept), [5. * ADVANCE]);
    let paragraphs = layout("AA\nAA AA", narrow, |_| {});
    assert_eq!(widths(&paragraphs).len(), 3, "newlines still end lines");
}

#[test]
fn truncation_drops_lines_that_do_not_fit_and_overflow_keeps_them() {
    let three = "A\nA\nA";
    let truncated = layout(three, Vec2::new(100., 45.), |_| {});
    assert_eq!(truncated.lines.len(), 2, "two 20 px lines fit 45 px");
    assert_eq!(truncated.glyphs.len(), 2);
    let kept = layout(three, Vec2::new(100., 45.), |run| {
        run.vertical_overflow = VerticalOverflow::Overflow;
    });
    assert_eq!(kept.lines.len(), 3);
    let none = layout("A", Vec2::new(100., 19.), |_| {});
    assert!(none.lines.is_empty() && none.glyphs.is_empty());
}

#[test]
fn glyphs_outside_the_rectangle_are_drawn_whole() {
    let wide = layout("AAAAAAAAAA", Vec2::new(50., 100.), |run| {
        run.horizontal_overflow = HorizontalOverflow::Overflow;
    });
    assert_eq!(wide.glyphs.len(), 10);
    let last = wide.glyphs.last().unwrap();
    assert!(last.rect_max.x > 50., "{last:?}");
    let first = &wide.glyphs[0];
    assert_eq!(
        last.rect_max - last.rect_min,
        first.rect_max - first.rect_min,
        "no glyph is clipped"
    );
}
