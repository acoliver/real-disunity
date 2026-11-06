use super::*;
use worldrunner_world_simulation::{PointerButton, PointerEvent};

fn point(w: &World, r: &UiRouter, object: ObjectHandle, x: f32) -> [f64; 2] {
    let resolved = layout(w, r);
    let rect = resolved.rect(object).unwrap();
    let position = rect.drawn_point(
        glam::Vec3::new(x, rect.size.y * 0.5, 0.) - (rect.pivot * rect.size).extend(0.),
    );
    [f64::from(position.x), 300. - f64::from(position.y)]
}
fn pointer(w: &World, r: &mut UiRouter, event: PointerEvent) {
    r.pointer(w, &layout(w, r), event).unwrap();
}
fn button(w: &World, r: &mut UiRouter, pressed: bool) {
    pointer(
        w,
        r,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed,
        },
    );
}
#[test]
fn click_places_caret_at_nearest_cached_boundary_at_display_scales() {
    for scale in [1., 2.] {
        let (w, mut r, fonts, object) = fixture(30);
        r.layout.display_scale = scale;
        text(&w, &mut r, &fonts, &TextInput::Commit("ABAB".into()));
        for (x, expected) in [(0., 0), (6., 1), (14., 2), (24., 3), (90., 4)] {
            let p = point(&w, &r, object, x);
            pointer(&w, &mut r, PointerEvent::Moved(p));
            button(&w, &mut r, true);
            assert_eq!(r.text_fields[&object].cursor(), expected);
            button(&w, &mut r, false);
        }
    }
}
#[test]
fn captured_drag_scrolls_and_shift_click_extends() {
    let (w, mut r, fonts, object) = fixture(40);
    text(
        &w,
        &mut r,
        &fonts,
        &TextInput::Commit("ABABABABABABABABABAB".into()),
    );
    key(&w, &mut r, UiKey::Home);
    let start = point(&w, &r, object, 0.);
    pointer(&w, &mut r, PointerEvent::Moved(start));
    button(&w, &mut r, true);
    for _ in 0..5 {
        let outside = point(&w, &r, object, 160.);
        pointer(&w, &mut r, PointerEvent::Moved(outside));
    }
    assert_eq!(r.text_fields[&object].selection(), 0..20);
    button(&w, &mut r, false);
    let left = point(&w, &r, object, 0.);
    pointer(&w, &mut r, PointerEvent::Moved(left));
    button(&w, &mut r, true);
    let scrolled = r.text_fields[&object].cursor();
    assert!(scrolled > 0 && scrolled < 20);
    button(&w, &mut r, false);
    key(&w, &mut r, UiKey::Shift);
    let end = point(&w, &r, object, 99.);
    pointer(&w, &mut r, PointerEvent::Moved(end));
    button(&w, &mut r, true);
    assert_eq!(r.text_fields[&object].selection(), scrolled..20);
}
#[test]
fn timed_double_click_selects_whitespace_delimited_word() {
    let (w, mut r, fonts, object) = fixture(20);
    text(&w, &mut r, &fonts, &TextInput::Commit("AB AB".into()));
    let p = point(&w, &r, object, 1.);
    pointer(&w, &mut r, PointerEvent::Moved(p));
    for millis in [100, 200] {
        for pressed in [true, false] {
            r.pointer_at(
                &w,
                &layout(&w, &r),
                PointerEvent::Button {
                    button: PointerButton::Left,
                    pressed,
                },
                std::time::Duration::from_millis(millis),
            )
            .unwrap();
        }
    }
    assert_eq!(r.text_fields[&object].selection(), 0..2);
}
#[test]
fn clipboard_copy_cut_paste_and_atomic_rejections() {
    let (w, mut r, fonts, object) = fixture(5);
    let mut clipboard = MemoryClipboard::default();
    text(&w, &mut r, &fonts, &TextInput::Commit("AB AB".into()));
    key(&w, &mut r, UiKey::SelectAll);
    for command in [
        ClipboardCommand::Copy,
        ClipboardCommand::Cut,
        ClipboardCommand::Paste,
    ] {
        r.clipboard(&w, &layout(&w, &r), &fonts, command, &mut clipboard)
            .unwrap();
        assert_eq!(clipboard.read_text().unwrap(), "AB AB");
        if command == ClipboardCommand::Cut {
            assert_eq!(r.text_fields[&object].value(), "");
        }
    }
    assert_eq!(r.text_fields[&object].value(), "AB AB");
    for (value, reason) in [
        ("A", TextReject::MaxLength),
        ("é", TextReject::Font(UiError::UnsupportedShaping('é'))),
        ("Z", TextReject::Font(UiError::MissingGlyph('Z'))),
        ("A\nB", TextReject::Font(UiError::UnsupportedShaping('\n'))),
    ] {
        if value != "A" {
            key(&w, &mut r, UiKey::SelectAll);
        }
        clipboard.write_text(value).unwrap();
        let response = r
            .clipboard(
                &w,
                &layout(&w, &r),
                &fonts,
                ClipboardCommand::Paste,
                &mut clipboard,
            )
            .unwrap();
        assert_eq!(
            response.actions,
            [UiAction::TextRejected {
                control: object,
                reason
            }]
        );
        assert_eq!(r.text_fields[&object].value(), "AB AB");
    }
}
#[test]
fn unavailable_clipboard_does_not_cut_selection() {
    struct Missing;
    impl Clipboard for Missing {
        fn read_text(&mut self) -> Result<String, ClipboardError> {
            Err(ClipboardError::Unavailable)
        }
        fn write_text(&mut self, _: &str) -> Result<(), ClipboardError> {
            Err(ClipboardError::Unavailable)
        }
    }
    let (w, mut r, fonts, object) = fixture(5);
    text(&w, &mut r, &fonts, &TextInput::Commit("AB".into()));
    key(&w, &mut r, UiKey::SelectAll);
    for command in [
        ClipboardCommand::Copy,
        ClipboardCommand::Cut,
        ClipboardCommand::Paste,
    ] {
        let response = r
            .clipboard(&w, &layout(&w, &r), &fonts, command, &mut Missing)
            .unwrap();
        assert_eq!(
            response.actions,
            [UiAction::TextRejected {
                control: object,
                reason: TextReject::Clipboard(ClipboardError::Unavailable)
            }]
        );
        assert_eq!(r.text_fields[&object].value(), "AB");
        assert_eq!(r.text_fields[&object].selection(), 0..2);
    }
}

#[test]
fn scrolled_scaled_hit_and_draw_share_character_positions() {
    for scale in [1., 2.] {
        let (w, mut r, fonts, object) = fixture(40);
        r.layout.display_scale = scale;
        text(
            &w,
            &mut r,
            &fonts,
            &TextInput::Commit("ABABABABABABABABABAB".into()),
        );
        let click = point(&w, &r, object, 10.);
        pointer(&w, &mut r, PointerEvent::Moved(click));
        button(&w, &mut r, true);
        assert_eq!(r.text_fields[&object].cursor(), 9);
        let mut plan = contribution::extract_ui_layout(&w, &fonts, layout(&w, &r)).unwrap();
        r.text_fields_plan(
            &w,
            &fonts,
            &mut plan,
            TextFieldStyle {
                caret: [1.; 4],
                selection: [1.; 4],
            },
        )
        .unwrap();
        let caret_x = plan
            .graphics
            .last()
            .unwrap()
            .graphic
            .quads
            .last()
            .unwrap()
            .corners[0][0];
        let expected = point(&w, &r, object, 6.6)[0];
        assert!((f64::from(caret_x) - expected).abs() < 0.001);
    }
}

#[test]
fn drag_selects_exact_range_and_capture_ends_on_release_or_cancel() {
    let (w, mut r, fonts, object) = fixture(20);
    text(&w, &mut r, &fonts, &TextInput::Commit("ABAB".into()));
    let start = point(&w, &r, object, 8.4);
    pointer(&w, &mut r, PointerEvent::Moved(start));
    button(&w, &mut r, true);
    let end = point(&w, &r, object, 25.2);
    pointer(&w, &mut r, PointerEvent::Moved(end));
    assert_eq!(r.text_fields[&object].selection(), 1..3);
    let outside = point(&w, &r, object, -50.);
    let response = r
        .pointer(&w, &layout(&w, &r), PointerEvent::Moved(outside))
        .unwrap();
    assert!(response.consumed);
    assert_eq!(r.text_fields[&object].selection(), 0..1);
    button(&w, &mut r, false);
    pointer(&w, &mut r, PointerEvent::Moved(end));
    assert_eq!(r.text_fields[&object].selection(), 0..1);
    button(&w, &mut r, true);
    pointer(&w, &mut r, PointerEvent::Cancel);
    pointer(&w, &mut r, PointerEvent::Moved(start));
    assert_eq!(r.text_fields[&object].selection(), 3..3);
}
