#![cfg(feature = "ui")]
#[path = "ui_accessibility/support.rs"]
mod support;
use support::{fixture, layout, rect};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{ObjectHandle, PointerButton, PointerEvent, World};

fn tree(w: &World, r: &mut UiRouter) -> UiSemanticTree {
    r.semantic_tree(w, &layout(w, r)).unwrap()
}
fn node(t: &UiSemanticTree, object: ObjectHandle) -> &UiSemanticNode {
    &t.nodes[&UiNodeId::Object(object)]
}
fn key(w: &World, r: &mut UiRouter, k: UiKey) {
    let l = layout(w, r);
    r.key(w, &l, k, false, false).unwrap();
    r.key(w, &l, k, true, false).unwrap();
}
fn request(w: &World, r: &mut UiRouter, o: ObjectHandle, action: UiAccessAction) -> UiResponse {
    r.accessibility_action(
        w,
        &layout(w, r),
        UiAccessRequest {
            target: UiNodeId::Object(o),
            action,
        },
    )
    .unwrap()
}
#[test]
fn menu_tree_roles_order_and_physical_bounds() {
    let (mut w, mut r, root, button) = fixture();
    let list = rect(&mut w, Some(root), -40.);
    let item = rect(&mut w, Some(list), 0.);
    r.controls.insert(item, UiControl::list_item(list, 0));
    let t = tree(&w, &mut r);
    assert_eq!(t.nodes[&UiNodeId::Window].role, UiRole::Window);
    assert_eq!(
        node(&t, root).children,
        [UiNodeId::Object(button), UiNodeId::Object(list)]
    );
    assert_eq!(node(&t, button).role, UiRole::Button);
    assert_eq!(node(&t, button).label, "Play");
    assert_eq!(
        node(&t, button).bounds.map(f64::to_bits),
        [150_f64, 140., 250., 160.].map(f64::to_bits)
    );
    assert_eq!(node(&t, list).role, UiRole::List);
    assert_eq!(node(&t, item).role, UiRole::ListItem);
    assert!(node(&t, button).actions.contains(&UiAccessAction::Press));
}
#[test]
fn ids_survive_label_and_value_changes_and_updates_are_incremental() {
    let (w, mut r, _, button) = fixture();
    let first = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(first.changed.len(), 3);
    r.semantics.get_mut(&button).unwrap().label = Some("Continue".into());
    let update = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(update.changed.len(), 1);
    assert_eq!(update.changed[0].id, UiNodeId::Object(button));
    assert_eq!(update.changed[0].label, "Continue");
    assert!(update.removed.is_empty());
    assert!(r.semantic_update(&w, &layout(&w, &r)).unwrap().is_empty());
}
#[test]
fn semantic_focus_tracks_keyboard_and_modal_restore_without_pointer_event() {
    let (mut w, mut r, root, button) = fixture();
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(
        tree(&w, &mut r).focus,
        UiNodeId::Object(r.focused().unwrap())
    );
    let dialog = rect(&mut w, Some(root), 30.);
    let close = rect(&mut w, Some(dialog), 0.);
    r.controls.insert(close, UiControl::button());
    r.modals.insert(dialog);
    let t = tree(&w, &mut r);
    assert!(!t.nodes.contains_key(&UiNodeId::Object(button)));
    assert!(!t.nodes.contains_key(&UiNodeId::Object(root)));
    assert_eq!(node(&t, dialog).role, UiRole::Dialog);
    assert_eq!(
        t.nodes[&UiNodeId::Window].children,
        [UiNodeId::Object(dialog)]
    );
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(tree(&w, &mut r).focus, UiNodeId::Object(close));
    assert!(
        request(&w, &mut r, button, UiAccessAction::Press)
            .actions
            .is_empty()
    );
    r.modals.remove(&dialog);
    assert_eq!(tree(&w, &mut r).focus, UiNodeId::Object(button));
    assert_eq!(r.focused(), Some(button));
}
#[test]
fn values_and_selection_come_from_router_state() {
    let (mut w, mut r, root, button) = fixture();
    r.controls.insert(button, UiControl::toggle(false));
    let slider = rect(&mut w, Some(root), 40.);
    r.controls.insert(
        slider,
        UiControl::slider(UiSlider::new(0., 10., Some(2.), 4., UiAxis::Horizontal).unwrap()),
    );
    let before = tree(&w, &mut r);
    assert_eq!(node(&before, button).value, UiSemanticValue::Toggled(false));
    request(&w, &mut r, button, UiAccessAction::Toggle);
    request(&w, &mut r, slider, UiAccessAction::SetValue(7.));
    let after = tree(&w, &mut r);
    assert_eq!(node(&after, button).id, node(&before, button).id);
    assert_eq!(node(&after, button).value, UiSemanticValue::Toggled(true));
    assert_eq!(
        node(&after, slider).value,
        UiSemanticValue::Slider {
            value: 8.,
            min: 0.,
            max: 10.,
            step: Some(2.),
        }
    );
    assert_eq!(
        request(&w, &mut r, slider, UiAccessAction::Increment).actions,
        [UiAction::ValueChanged {
            control: slider,
            value: 10.
        }]
    );
    assert!(
        request(&w, &mut r, slider, UiAccessAction::SetValue(f64::NAN))
            .actions
            .is_empty()
    );
    let item = rect(&mut w, Some(root), -40.);
    r.controls.insert(item, UiControl::list_item(root, 3));
    request(&w, &mut r, item, UiAccessAction::Select);
    assert_eq!(
        node(&tree(&w, &mut r), item).value,
        UiSemanticValue::Selected(true)
    );
}
#[test]
fn live_status_announces_once_per_change_not_unrelated_updates() {
    let (mut w, mut r, root, button) = fixture();
    let status = rect(&mut w, Some(root), 40.);
    r.semantics.insert(
        status,
        UiSemantics {
            label: Some("Ready".into()),
            live: true,
        },
    );
    let first = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(first.announcements.len(), 1);
    assert_eq!(first.announcements[0].text, "Ready");
    assert_eq!(node(&tree(&w, &mut r), status).role, UiRole::Status);
    key(&w, &mut r, UiKey::Tab);
    r.semantics.get_mut(&button).unwrap().label = Some("Start".into());
    assert!(
        r.semantic_update(&w, &layout(&w, &r))
            .unwrap()
            .announcements
            .is_empty()
    );
    r.semantics.get_mut(&status).unwrap().label = Some("Saved".into());
    let next = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(next.announcements.len(), 1);
    assert_eq!(next.announcements[0].text, "Saved");
    assert!(
        r.semantic_update(&w, &layout(&w, &r))
            .unwrap()
            .announcements
            .is_empty()
    );
}
#[test]
fn assistive_press_and_focus_use_the_existing_click_action() {
    let (w, mut r, _, button) = fixture();
    let l = layout(&w, &r);
    r.pointer(&w, &l, PointerEvent::Moved([200., 150.]))
        .unwrap();
    r.pointer(
        &w,
        &l,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true,
        },
    )
    .unwrap();
    let clicked = r
        .pointer(
            &w,
            &l,
            PointerEvent::Button {
                button: PointerButton::Left,
                pressed: false,
            },
        )
        .unwrap();
    assert_eq!(request(&w, &mut r, button, UiAccessAction::Press), clicked);
    request(&w, &mut r, button, UiAccessAction::Focus);
    assert_eq!(r.focused(), Some(button));
    r.controls.get_mut(&button).unwrap().enabled = false;
    assert!(
        request(&w, &mut r, button, UiAccessAction::Press)
            .actions
            .is_empty()
    );
    let t = tree(&w, &mut r);
    assert!(node(&t, button).disabled);
    assert!(node(&t, button).actions.is_empty());
}
