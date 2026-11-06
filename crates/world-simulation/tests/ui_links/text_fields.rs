use super::*;
use worldrunner_world_simulation::TextInput;

#[test]
fn links_fields_and_buttons_share_forward_and_reverse_focus_order() {
    let (mut w, mut r, fonts, root, text) = fixture();
    two_links(&mut w, text);
    let field = rect(&mut w, Some(root), Vec2::new(30., 20.));
    let button = rect(&mut w, Some(root), Vec2::ONE);
    r.controls.insert(root, UiControl::button());
    r.controls.insert(field, UiControl::text_field());
    r.controls.insert(button, UiControl::button());
    let font = w
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::Text(run) => Some(run.font),
            _ => None,
        })
        .unwrap();
    r.text_fields.insert(
        field,
        UiTextField::new(TextRun::new(font), 20, &fonts).unwrap(),
    );
    let order = [
        (root, None),
        (text, Some(UiLinkId(9))),
        (text, Some(UiLinkId(2))),
        (field, None),
        (button, None),
    ];
    for (object, link) in order {
        key(&w, &mut r, UiKey::Tab);
        assert_eq!(r.focused(), Some(object));
        assert_eq!(r.focused_link(), link.map(|id| (object, id)));
    }
    key(&w, &mut r, UiKey::Shift);
    for (object, link) in order.into_iter().rev().cycle().skip(1).take(order.len()) {
        key(&w, &mut r, UiKey::Tab);
        assert_eq!(r.focused(), Some(object));
        assert_eq!(r.focused_link(), link.map(|id| (object, id)));
    }
}

#[test]
fn leaving_field_for_link_cancels_preedit_and_keeps_link_activation() {
    let (w, mut r, fonts, root, text) = fixture();
    r.controls.insert(root, UiControl::text_field());
    let font = w
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::Text(run) => Some(run.font),
            _ => None,
        })
        .unwrap();
    r.text_fields.insert(
        root,
        UiTextField::new(TextRun::new(font), 20, &fonts).unwrap(),
    );
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(root));
    r.text(&w, &layout(&w, &r), &fonts, &TextInput::Commit("A".into()))
        .unwrap();
    r.text(
        &w,
        &layout(&w, &r),
        &fonts,
        &TextInput::Preedit {
            text: "B".into(),
            cursor: Some((1, 1)),
        },
    )
    .unwrap();
    assert_eq!(r.text_fields[&root].preedit(), "B");
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(7))));
    assert!(r.text_fields[&root].preedit().is_empty());
    assert_eq!(r.text_fields[&root].value(), "A");
    assert!(
        r.text_input_area(&w, &layout(&w, &r), &fonts)
            .unwrap()
            .is_none()
    );
    let tree = r.semantic_tree(&w, &layout(&w, &r)).unwrap();
    assert_eq!(tree.focus, UiNodeId::Object(text));
    assert_eq!(
        key(&w, &mut r, UiKey::Enter).actions,
        [UiAction::LinkActivated {
            control: text,
            link: UiLinkId(7),
        }]
    );
}
