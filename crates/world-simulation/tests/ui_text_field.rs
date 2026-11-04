#![cfg(feature = "ui")]
#[path = "ui_text_field/pointer_clipboard.rs"]
mod pointer_clipboard;
mod ui_font;
use glam::Vec2;
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, TextInput, World, WorldSettings,
};

fn fixture(max: usize) -> (World, UiRouter, FontBook, ObjectHandle) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let object = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        object,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: Vec2::new(100., 24.),
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_parent(object, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut bytes = ui_font::font();
    let record = bytes[12..]
        .chunks_exact(16)
        .position(|r| &r[..4] == b"cmap")
        .unwrap()
        * 16
        + 12;
    let cmap = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize;
    let delta = cmap + 412 + (usize::from(b'Z') - 32) * 2;
    bytes[delta..delta + 2].copy_from_slice(&0_u16.wrapping_sub(u16::from(b'Z')).to_be_bytes());
    let font = fonts.register(bytes).unwrap();
    let mut r = UiRouter::default();
    r.controls.insert(object, UiControl::text_field());
    r.text_fields.insert(
        object,
        UiTextField::new(TextRun::new(font), max, &fonts).unwrap(),
    );
    key(&w, &mut r, UiKey::Tab);
    (w, r, fonts, object)
}
fn layout(w: &World, r: &UiRouter) -> UiLayout {
    w.ui_layout_with([400, 300].into(), &r.layout).unwrap()
}
fn key(w: &World, r: &mut UiRouter, key: UiKey) -> UiResponse {
    r.key(w, &layout(w, r), key, false, false).unwrap();
    r.key(w, &layout(w, r), key, true, false).unwrap()
}
fn text(w: &World, r: &mut UiRouter, fonts: &FontBook, event: &TextInput) -> UiResponse {
    r.text(w, &layout(w, r), fonts, event).unwrap()
}
#[test]
fn preedit_draws_inline_without_committing_or_submitting() {
    let (w, mut r, fonts, object) = fixture(10);
    text(&w, &mut r, &fonts, &TextInput::Commit("AB".into()));
    text(
        &w,
        &mut r,
        &fonts,
        &TextInput::Preedit {
            text: "A".into(),
            cursor: Some((1, 1)),
        },
    );
    assert!(key(&w, &mut r, UiKey::Enter).actions.is_empty());
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
    assert_eq!(plan.texts[0].quads.len(), 3);
    assert_eq!(r.text_fields[&object].value(), "AB");
    text(&w, &mut r, &fonts, &TextInput::Cancel);
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
    assert_eq!(plan.texts[0].quads.len(), 2);
}
#[test]
fn committed_repeat_and_preedit_commit_insert_once_and_cancel_preserves_value() {
    let (w, mut r, fonts, object) = fixture(20);
    for _ in 0..3 {
        text(&w, &mut r, &fonts, &TextInput::Commit("A".into()));
    }
    text(
        &w,
        &mut r,
        &fonts,
        &TextInput::Preedit {
            text: "B".into(),
            cursor: Some((1, 1)),
        },
    );
    assert_eq!(r.text_fields[&object].value(), "AAA");
    text(&w, &mut r, &fonts, &TextInput::Commit("B".into()));
    assert_eq!(r.text_fields[&object].value(), "AAAB");
    text(
        &w,
        &mut r,
        &fonts,
        &TextInput::Preedit {
            text: "A".into(),
            cursor: None,
        },
    );
    text(&w, &mut r, &fonts, &TextInput::Cancel);
    assert_eq!(r.text_fields[&object].value(), "AAAB");
    assert!(r.text_fields[&object].preedit().is_empty());
}
#[test]
fn selection_typing_backspace_delete_and_word_movement() {
    let (w, mut r, fonts, object) = fixture(20);
    text(&w, &mut r, &fonts, &TextInput::Commit("AB AB".into()));
    key(&w, &mut r, UiKey::Control);
    key(&w, &mut r, UiKey::Left);
    assert_eq!(r.text_fields[&object].cursor(), 3);
    r.key(&w, &layout(&w, &r), UiKey::Control, false, false)
        .unwrap();
    key(&w, &mut r, UiKey::Shift);
    key(&w, &mut r, UiKey::End);
    assert_eq!(r.text_fields[&object].selection(), 3..5);
    text(&w, &mut r, &fonts, &TextInput::Commit("B".into()));
    assert_eq!(r.text_fields[&object].value(), "AB B");
    key(&w, &mut r, UiKey::Home);
    key(&w, &mut r, UiKey::Backspace);
    assert_eq!(r.text_fields[&object].value(), "");
    text(&w, &mut r, &fonts, &TextInput::Commit("AB".into()));
    r.key(&w, &layout(&w, &r), UiKey::Shift, false, false)
        .unwrap();
    key(&w, &mut r, UiKey::Home);
    key(&w, &mut r, UiKey::Delete);
    assert_eq!(r.text_fields[&object].value(), "B");
}
#[test]
fn rejects_length_unsupported_and_missing_glyph_atomically_and_submits() {
    let (w, mut r, fonts, object) = fixture(2);
    for (value, reason) in [
        ("AAA", TextReject::MaxLength),
        ("é", TextReject::Font(UiError::UnsupportedShaping('é'))),
        ("Z", TextReject::Font(UiError::MissingGlyph('Z'))),
    ] {
        assert_eq!(
            text(&w, &mut r, &fonts, &TextInput::Commit(value.into())).actions,
            [UiAction::TextRejected {
                control: object,
                reason
            }]
        );
        assert_eq!(r.text_fields[&object].value(), "");
    }
    text(&w, &mut r, &fonts, &TextInput::Commit("AB".into()));
    assert_eq!(
        key(&w, &mut r, UiKey::Enter).actions,
        [UiAction::TextSubmitted { control: object }]
    );
    assert_eq!(
        key(&w, &mut r, UiKey::Escape).actions,
        [UiAction::TextCancelled { control: object }]
    );
    assert_eq!(r.text_fields[&object].value(), "AB");
}
#[test]
fn modal_and_disabled_fields_block_text_and_ime_target() {
    let (mut w, mut r, fonts, object) = fixture(10);
    assert!(
        r.text_input_area(&w, &layout(&w, &r), &fonts)
            .unwrap()
            .is_some()
    );
    r.controls.get_mut(&object).unwrap().enabled = false;
    assert!(!text(&w, &mut r, &fonts, &TextInput::Commit("A".into())).consumed);
    assert_eq!(r.text_fields[&object].value(), "");
    assert!(
        r.text_input_area(&w, &layout(&w, &r), &fonts)
            .unwrap()
            .is_none()
    );
    r.controls.get_mut(&object).unwrap().enabled = true;
    key(&w, &mut r, UiKey::Tab);
    let root = w.parent(object).unwrap();
    let modal = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        modal,
        ComponentDesc::RectTransform(RectTransform::default()),
    )
    .unwrap();
    w.set_parent(modal, root, ParentSpace::KeepLocal).unwrap();
    r.modals.insert(modal);
    assert!(text(&w, &mut r, &fonts, &TextInput::Commit("B".into())).consumed);
    assert_eq!(r.text_fields[&object].value(), "");
    assert!(
        r.text_input_area(&w, &layout(&w, &r), &fonts)
            .unwrap()
            .is_none()
    );
}
#[test]
fn display_scale_keeps_drawn_button_and_physical_hits_aligned() {
    let (w, mut r, _, object) = fixture(10);
    r.controls.insert(object, UiControl::button());
    for scale in [1., 2.] {
        r.layout.display_scale = scale;
        let l = layout(&w, &r);
        let rect = l.rect(object).unwrap();
        let corners = rect.screen_corners.unwrap();
        assert_eq!(
            (corners[1].x - corners[0].x).to_bits(),
            (100_f32 * scale).to_bits()
        );
        let edge = Vec2::new(corners[1].x - 1., (corners[0].y + corners[2].y) * 0.5);
        r.pointer(
            &w,
            &l,
            worldrunner_world_simulation::PointerEvent::Moved([
                f64::from(edge.x),
                300. - f64::from(edge.y),
            ]),
        )
        .unwrap();
        for pressed in [true, false] {
            let out = r
                .pointer(
                    &w,
                    &l,
                    worldrunner_world_simulation::PointerEvent::Button {
                        button: worldrunner_world_simulation::PointerButton::Left,
                        pressed,
                    },
                )
                .unwrap();
            if !pressed {
                assert_eq!(out.actions, [UiAction::Activated { object }]);
            }
        }
    }
}
