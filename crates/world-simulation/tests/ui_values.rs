#![cfg(feature = "ui")]
#[path = "ui_values/edges.rs"]
mod edges;
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, PointerButton, PointerEvent, World,
    WorldSettings,
};

fn rect(w: &mut World, parent: Option<ObjectHandle>, at: Vec2) -> ObjectHandle {
    let object = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        object,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: Vec2::new(100., 20.),
            anchored_position: at,
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    object
}
fn fixture(control: UiControl) -> (World, UiRouter, ObjectHandle, ObjectHandle) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut w, None, Vec2::ZERO);
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let object = rect(&mut w, Some(root), Vec2::ZERO);
    let mut r = UiRouter::default();
    r.controls.insert(object, control);
    (w, r, root, object)
}
fn slider(step: Option<f64>) -> UiControl {
    UiControl::slider(UiSlider::new(-10., 10., step, 0., UiAxis::Horizontal).unwrap())
}
fn layout(w: &World) -> UiLayout {
    w.ui_layout(UVec2::new(400, 300)).unwrap()
}
fn pointer(w: &World, r: &mut UiRouter, event: PointerEvent) -> UiResponse {
    r.pointer(w, &layout(w), event).unwrap()
}
fn moved(w: &World, r: &mut UiRouter, x: f64, y: f64) -> UiResponse {
    pointer(w, r, PointerEvent::Moved([x, y]))
}
fn button(w: &World, r: &mut UiRouter, pressed: bool) -> UiResponse {
    pointer(
        w,
        r,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed,
        },
    )
}
fn key(w: &World, r: &mut UiRouter, k: UiKey) -> UiResponse {
    r.key(w, &layout(w), k, false, false).unwrap();
    r.key(w, &layout(w), k, true, false).unwrap()
}
fn changed(control: ObjectHandle, value: f64) -> Vec<UiAction> {
    vec![UiAction::ValueChanged { control, value }]
}
#[test]
fn slider_track_press_updates_immediately_and_release_does_not_activate() {
    let (w, mut r, _, object) = fixture(slider(None));
    moved(&w, &mut r, 225., 150.);
    assert_eq!(button(&w, &mut r, true).actions, changed(object, 5.));
    assert_eq!(r.focused(), Some(object));
    assert_eq!(r.slider(object).unwrap().value().to_bits(), 5_f64.to_bits());
    assert_eq!(
        r.slider(object).unwrap().normalized_value().to_bits(),
        0.75_f64.to_bits()
    );
    assert!(button(&w, &mut r, false).actions.is_empty());
    assert!(button(&w, &mut r, false).actions.is_empty());
}
#[test]
fn slider_capture_drags_past_ends_and_suppresses_unchanged_values() {
    let (w, mut r, _, object) = fixture(slider(None));
    moved(&w, &mut r, 200., 150.);
    assert!(button(&w, &mut r, true).actions.is_empty());
    assert_eq!(moved(&w, &mut r, 225., 0.).actions, changed(object, 5.));
    assert_eq!(moved(&w, &mut r, 900., 150.).actions, changed(object, 10.));
    let out = moved(&w, &mut r, 1000., 150.);
    assert!(out.consumed && out.actions.is_empty());
    assert_eq!(
        moved(&w, &mut r, -900., 150.).actions,
        changed(object, -10.)
    );
    assert!(button(&w, &mut r, false).actions.is_empty());
    assert!(moved(&w, &mut r, 200., 150.).actions.is_empty());
    assert_eq!(
        r.slider(object).unwrap().value().to_bits(),
        (-10_f64).to_bits()
    );
}
#[test]
fn slider_snaps_from_min_and_keeps_both_range_ends_reachable() {
    let control =
        UiControl::slider(UiSlider::new(1., 10., Some(4.), -30., UiAxis::Horizontal).unwrap());
    let (w, mut r, _, object) = fixture(control);
    assert_eq!(r.slider(object).unwrap().value().to_bits(), 1_f64.to_bits());
    moved(&w, &mut r, 210., 150.);
    assert_eq!(button(&w, &mut r, true).actions, changed(object, 5.));
    assert!(moved(&w, &mut r, 211., 150.).actions.is_empty());
    assert_eq!(moved(&w, &mut r, 500., 150.).actions, changed(object, 10.));
    assert_eq!(moved(&w, &mut r, 0., 150.).actions, changed(object, 1.));
    assert_eq!(
        UiSlider::new(1., 10., Some(4.), 100., UiAxis::Horizontal)
            .unwrap()
            .value()
            .to_bits(),
        10_f64.to_bits()
    );
}
#[test]
fn slider_arrows_home_end_and_navigation_repeats_change_only_the_value() {
    let (w, mut r, _, object) = fixture(slider(Some(2.)));
    key(&w, &mut r, UiKey::Tab);
    for (k, value) in [
        (UiKey::Right, 2.),
        (UiKey::Up, 4.),
        (UiKey::Left, 2.),
        (UiKey::Down, 0.),
        (UiKey::Home, -10.),
        (UiKey::End, 10.),
    ] {
        assert_eq!(key(&w, &mut r, k).actions, changed(object, value));
        assert_eq!(r.focused(), Some(object));
    }
    assert!(key(&w, &mut r, UiKey::Right).actions.is_empty());
    assert!(key(&w, &mut r, UiKey::End).actions.is_empty());
    assert_eq!(key(&w, &mut r, UiKey::Left).actions, changed(object, 8.));
    assert_eq!(
        r.key(&w, &layout(&w), UiKey::Left, true, true)
            .unwrap()
            .actions,
        changed(object, 6.)
    );
    assert!(
        r.key(&w, &layout(&w), UiKey::Left, true, false)
            .unwrap()
            .actions
            .is_empty()
    );
    for k in [UiKey::Enter, UiKey::Space] {
        assert!(key(&w, &mut r, k).actions.is_empty());
    }
}
#[test]
fn continuous_keyboard_step_is_one_percent_of_the_range() {
    let (w, mut r, _, object) = fixture(slider(None));
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(key(&w, &mut r, UiKey::Right).actions, changed(object, 0.2));
    assert_eq!(key(&w, &mut r, UiKey::Left).actions, changed(object, 0.));
}
#[test]
fn vertical_slider_uses_local_up_even_for_rotated_tracks() {
    let control = UiControl::slider(UiSlider::new(0., 1., None, 0., UiAxis::Vertical).unwrap());
    let (mut w, mut r, _, object) = fixture(control);
    moved(&w, &mut r, 200., 145.);
    assert_eq!(button(&w, &mut r, true).actions, changed(object, 0.75));
    assert_eq!(moved(&w, &mut r, 200., 0.).actions, changed(object, 1.));
    button(&w, &mut r, false);
    w.set_local_transform(
        object,
        worldrunner_world_simulation::LocalTransform {
            rotation: glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    )
    .unwrap();
    moved(&w, &mut r, 200., 150.);
    assert_eq!(button(&w, &mut r, true).actions, changed(object, 0.5));
    assert_eq!(moved(&w, &mut r, 190., 150.).actions, changed(object, 1.));
}
#[test]
fn slider_cancel_disable_removal_and_group_flags_stop_dragging() {
    let (mut w, mut r, root, object) = fixture(slider(None));
    moved(&w, &mut r, 200., 150.);
    button(&w, &mut r, true);
    pointer(&w, &mut r, PointerEvent::Cancel);
    assert!(moved(&w, &mut r, 225., 150.).actions.is_empty());
    button(&w, &mut r, true);
    r.controls.get_mut(&object).unwrap().enabled = false;
    assert!(moved(&w, &mut r, 230., 150.).actions.is_empty());
    r.controls.get_mut(&object).unwrap().enabled = true;
    assert!(moved(&w, &mut r, 240., 150.).actions.is_empty());
    button(&w, &mut r, false);
    button(&w, &mut r, true);
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
    assert!(moved(&w, &mut r, 200., 150.).actions.is_empty());
    r.controls.remove(&object);
    assert!(moved(&w, &mut r, 210., 150.).actions.is_empty());
}
#[test]
fn disabled_value_controls_ignore_pointer_keyboard_and_focus() {
    for control in [slider(Some(2.)), UiControl::toggle(false)] {
        let (w, mut r, _, object) = fixture(control);
        r.controls.get_mut(&object).unwrap().enabled = false;
        moved(&w, &mut r, 225., 150.);
        let out = button(&w, &mut r, true);
        assert!(out.consumed && out.actions.is_empty());
        assert!(button(&w, &mut r, false).actions.is_empty());
        for k in [
            UiKey::Tab,
            UiKey::Right,
            UiKey::End,
            UiKey::Space,
            UiKey::Enter,
        ] {
            assert!(key(&w, &mut r, k).actions.is_empty());
            assert_eq!(r.focused(), None);
        }
        assert_eq!(r.controls[&object].kind, control.kind);
    }
}
#[test]
fn toggle_click_requires_inside_release_and_toggles_once() {
    let (w, mut r, _, object) = fixture(UiControl::toggle(false));
    moved(&w, &mut r, 200., 150.);
    assert!(button(&w, &mut r, true).actions.is_empty());
    assert!(button(&w, &mut r, true).actions.is_empty());
    assert_eq!(
        button(&w, &mut r, false).actions,
        [UiAction::Toggled {
            control: object,
            on: true
        }]
    );
    assert_eq!(r.controls[&object].kind, UiControlKind::Toggle { on: true });
    assert!(button(&w, &mut r, false).actions.is_empty());
    button(&w, &mut r, true);
    moved(&w, &mut r, 0., 0.);
    assert!(button(&w, &mut r, false).actions.is_empty());
    moved(&w, &mut r, 200., 150.);
    button(&w, &mut r, true);
    assert_eq!(
        button(&w, &mut r, false).actions,
        [UiAction::Toggled {
            control: object,
            on: false
        }]
    );
}
#[test]
fn toggle_space_enter_and_repeats_use_the_same_state() {
    let (w, mut r, _, object) = fixture(UiControl::toggle(false));
    key(&w, &mut r, UiKey::Tab);
    for (k, on) in [(UiKey::Space, true), (UiKey::Enter, false)] {
        assert_eq!(
            key(&w, &mut r, k).actions,
            [UiAction::Toggled {
                control: object,
                on
            }]
        );
        assert!(
            r.key(&w, &layout(&w), k, true, true)
                .unwrap()
                .actions
                .is_empty()
        );
        assert!(
            r.key(&w, &layout(&w), k, true, false)
                .unwrap()
                .actions
                .is_empty()
        );
    }
}
#[test]
fn value_controls_follow_tab_order_and_modal_restrictions() {
    let (mut w, mut r, root, slider) = fixture(slider(None));
    let toggle = rect(&mut w, Some(root), Vec2::new(0., 50.));
    r.controls.insert(toggle, UiControl::toggle(false));
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(slider));
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(toggle));
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(slider));
    r.modals.insert(toggle);
    moved(&w, &mut r, 225., 150.);
    assert!(button(&w, &mut r, true).actions.is_empty());
    assert!(button(&w, &mut r, false).actions.is_empty());
    assert!(key(&w, &mut r, UiKey::End).actions.is_empty());
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(toggle));
    assert_eq!(
        key(&w, &mut r, UiKey::Space).actions,
        [UiAction::Toggled {
            control: toggle,
            on: true
        }]
    );
    r.modals.remove(&toggle);
    key(&w, &mut r, UiKey::Shift);
    assert_eq!(r.focused(), Some(slider));
    r.modals.insert(slider);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(slider));
    assert_eq!(key(&w, &mut r, UiKey::End).actions, changed(slider, 10.));
    moved(&w, &mut r, 200., 100.);
    assert!(button(&w, &mut r, true).actions.is_empty());
    assert!(button(&w, &mut r, false).actions.is_empty());
}
#[test]
fn invalid_slider_configuration_fails_at_construction() {
    for (min, max, step, value) in [
        (1., 1., None, 1.),
        (2., 1., None, 1.),
        (0., 1., Some(0.), 0.),
        (0., 1., Some(-1.), 0.),
        (0., 1., Some(f64::NAN), 0.),
        (0., 1., Some(f64::INFINITY), 0.),
        (f64::NEG_INFINITY, 1., None, 0.),
        (0., f64::INFINITY, None, 0.),
        (0., 1., None, f64::NAN),
        (0., 1., None, f64::INFINITY),
    ] {
        assert!(UiSlider::new(min, max, step, value, UiAxis::Horizontal).is_err());
    }
}
