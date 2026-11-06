#![cfg(feature = "ui")]
#[path = "ui_accessibility/support.rs"]
mod support;
mod ui_font;
use support::{fixture, layout, rect};
use worldrunner_world_simulation::ui::*;

#[test]
fn restoring_a_live_region_after_a_modal_does_not_repeat_the_announcement() {
    let (mut w, mut r, root, _) = fixture();
    let status = rect(&mut w, Some(root), 40.);
    r.semantics.insert(
        status,
        UiSemantics {
            label: Some("Ready".into()),
            live: true,
        },
    );
    assert_eq!(
        r.semantic_update(&w, &layout(&w, &r))
            .unwrap()
            .announcements
            .len(),
        1
    );
    let dialog = rect(&mut w, Some(root), 60.);
    r.modals.insert(dialog);
    r.semantic_update(&w, &layout(&w, &r)).unwrap();
    r.modals.remove(&dialog);
    assert!(
        r.semantic_update(&w, &layout(&w, &r))
            .unwrap()
            .announcements
            .is_empty()
    );
    r.semantics.get_mut(&status).unwrap().label = Some("Saved".into());
    assert_eq!(
        r.semantic_update(&w, &layout(&w, &r))
            .unwrap()
            .announcements
            .len(),
        1
    );
}

#[test]
fn text_components_supply_names_and_live_text_tracks_the_component() {
    use worldrunner_world_simulation::ComponentDesc;
    let (mut w, mut r, _, button) = fixture();
    r.semantics.remove(&button);
    let label = rect(&mut w, Some(button), 0.);
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let component = w
        .add_component(
            label,
            ComponentDesc::Text(TextRun {
                text: "Start".into(),
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    let tree = r.semantic_tree(&w, &layout(&w, &r)).unwrap();
    assert_eq!(tree.nodes[&UiNodeId::Object(button)].label, "Start");
    assert_eq!(tree.nodes[&UiNodeId::Object(label)].role, UiRole::Text);
    r.semantics.insert(
        label,
        UiSemantics {
            label: None,
            live: true,
        },
    );
    r.semantic_update(&w, &layout(&w, &r)).unwrap();
    w.set_component(
        component,
        ComponentDesc::Text(TextRun {
            text: "Saved".into(),
            ..TextRun::new(font)
        }),
    )
    .unwrap();
    let update = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(update.announcements[0].text, "Saved");
    assert!(
        r.semantic_update(&w, &layout(&w, &r))
            .unwrap()
            .announcements
            .is_empty()
    );
}

#[test]
fn hidden_disabled_and_removed_controls_cannot_be_activated() {
    use worldrunner_world_simulation::ComponentDesc;
    let (mut w, mut r, root, button) = fixture();
    w.add_component(
        root,
        ComponentDesc::CanvasGroup(CanvasGroup {
            flags: CanvasGroupFlags {
                interactable: false,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap();
    let request = UiAccessRequest {
        target: UiNodeId::Object(button),
        action: UiAccessAction::Press,
    };
    assert!(
        !r.accessibility_action(&w, &layout(&w, &r), request)
            .unwrap()
            .consumed
    );
    assert!(r.semantic_tree(&w, &layout(&w, &r)).unwrap().nodes[&request.target].disabled);
    r.semantic_update(&w, &layout(&w, &r)).unwrap();
    w.destroy_object(button).unwrap();
    let update = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(update.removed, [request.target]);
    assert!(
        !r.accessibility_action(&w, &layout(&w, &r), request)
            .unwrap()
            .consumed
    );
}
