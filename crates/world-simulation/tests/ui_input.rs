#![cfg(feature = "ui")]
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, PointerButton, PointerEvent, World,
    WorldSettings,
};
fn ignore_parent_groups(w: &mut World, object: ObjectHandle) {
    w.add_component(
        object,
        ComponentDesc::CanvasGroup(CanvasGroup {
            flags: CanvasGroupFlags {
                ignore_parent_groups: true,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap();
}

fn rect(w: &mut World, parent: Option<ObjectHandle>, x: f32) -> ObjectHandle {
    let o = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        o,
        ComponentDesc::RectTransform(RectTransform {
            anchored_position: Vec2::new(x, 0.),
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_parent(o, parent, ParentSpace::KeepLocal).unwrap();
    o
}
fn fixture() -> (World, UiRouter, ObjectHandle, ObjectHandle) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut w, None, 0.);
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let button = rect(&mut w, Some(root), 0.);
    let mut router = UiRouter::default();
    router.controls.insert(button, UiControl::button());
    (w, router, root, button)
}
fn send(w: &World, router: &mut UiRouter, event: PointerEvent) -> UiResponse {
    router
        .pointer(w, &w.ui_layout(UVec2::new(800, 600)).unwrap(), event)
        .unwrap()
}
fn at(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Moved([x, y])
}
fn button(pressed: bool) -> PointerEvent {
    PointerEvent::Button {
        button: PointerButton::Left,
        pressed,
    }
}
fn click(w: &World, r: &mut UiRouter, x: f64, y: f64) -> UiResponse {
    send(w, r, at(x, y));
    send(w, r, button(true));
    send(w, r, button(false))
}
#[test]
fn release_inside_activates_once_and_consumes_hits_not_misses() {
    let (w, mut r, _, target) = fixture();
    assert!(!send(&w, &mut r, at(10., 10.)).consumed);
    let out = click(&w, &mut r, 400., 300.);
    assert!(out.consumed);
    assert_eq!(out.actions, vec![UiAction::Activated { object: target }]);
    assert!(send(&w, &mut r, button(false)).actions.is_empty());
}
#[test]
fn outside_press_or_outside_release_cannot_activate() {
    let (w, mut r, _, _) = fixture();
    send(&w, &mut r, at(10., 10.));
    send(&w, &mut r, button(true));
    send(&w, &mut r, at(400., 300.));
    assert!(send(&w, &mut r, button(false)).actions.is_empty());
    send(&w, &mut r, button(true));
    assert!(send(&w, &mut r, at(10., 10.)).consumed);
    let out = send(&w, &mut r, button(false));
    assert!(out.consumed);
    assert!(out.actions.is_empty());
}
#[test]
fn focus_cancel_prevents_synthetic_release_activation() {
    let (w, mut r, _, _) = fixture();
    send(&w, &mut r, at(400., 300.));
    send(&w, &mut r, button(true));
    assert!(send(&w, &mut r, PointerEvent::Cancel).consumed);
    assert!(send(&w, &mut r, button(false)).actions.is_empty());
}
#[test]
fn canvas_sort_order_wins_over_creation_order() {
    let (mut w, mut r, root, top) = fixture();
    let canvas = w
        .published_components()
        .find(|c| c.object == root && matches!(c.desc, ComponentDesc::Canvas(_)))
        .unwrap()
        .handle;
    w.set_component(
        canvas,
        ComponentDesc::Canvas(Canvas {
            sorting_layer: 2,
            sorting_order: -1,
            ..Default::default()
        }),
    )
    .unwrap();
    let back_root = rect(&mut w, None, 0.);
    w.add_component(
        back_root,
        ComponentDesc::Canvas(Canvas {
            sorting_layer: 1,
            sorting_order: 99,
            ..Default::default()
        }),
    )
    .unwrap();
    let back = rect(&mut w, Some(back_root), 0.);
    r.controls.insert(back, UiControl::button());
    assert_eq!(
        click(&w, &mut r, 400., 300.).actions,
        vec![UiAction::Activated { object: top }]
    );
}
#[test]
fn hierarchy_order_places_descendants_before_later_siblings() {
    let (mut w, mut r, root, first) = fixture();
    let later = rect(&mut w, Some(root), 0.);
    let child = rect(&mut w, Some(first), 0.);
    r.controls.insert(later, UiControl::button());
    r.controls.insert(child, UiControl::button());
    let layout = w.ui_layout(UVec2::new(800, 600)).unwrap();
    assert!(
        layout.rect(first).unwrap().hierarchy_order < layout.rect(child).unwrap().hierarchy_order
    );
    assert!(
        layout.rect(child).unwrap().hierarchy_order < layout.rect(later).unwrap().hierarchy_order
    );
    assert_eq!(
        click(&w, &mut r, 400., 300.).actions,
        vec![UiAction::Activated { object: later }]
    );
}
#[test]
fn disabled_and_noninteractable_block_without_activating() {
    let (mut w, mut r, root, target) = fixture();
    r.controls.get_mut(&target).unwrap().enabled = false;
    let out = click(&w, &mut r, 400., 300.);
    assert!(out.consumed && out.actions.is_empty());
    r.controls.get_mut(&target).unwrap().enabled = true;
    let group = w
        .add_component(
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
    let out = click(&w, &mut r, 400., 300.);
    assert!(out.consumed && out.actions.is_empty());
    assert!(!click(&w, &mut r, 10., 10.).consumed);
    w.set_component(
        group,
        ComponentDesc::CanvasGroup(CanvasGroup {
            flags: CanvasGroupFlags {
                blocks_raycasts: false,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(!click(&w, &mut r, 400., 300.).consumed);
}
#[test]
fn modal_blocks_the_board_and_controls_outside_its_subtree() {
    let (mut w, mut r, root, target) = fixture();
    let modal = rect(&mut w, Some(root), 200.);
    r.modals.insert(modal);
    let out = click(&w, &mut r, 400., 300.);
    assert!(out.consumed && out.actions.is_empty());
    r.controls.insert(modal, UiControl::choice(root, 3));
    assert_eq!(
        click(&w, &mut r, 600., 300.).actions,
        vec![UiAction::ChoiceSelected {
            group: root,
            index: 3
        }]
    );
    assert_eq!(r.selected(root), Some(3));
    w.set_active(modal, false).unwrap();
    assert_eq!(
        click(&w, &mut r, 400., 300.).actions,
        vec![UiAction::Activated { object: target }]
    );
}
#[test]
fn two_x_scale_uses_physical_pixels_and_top_left_origin() {
    let (mut w, mut r, root, target) = fixture();
    let canvas = w
        .published_components()
        .find(|c| c.object == root && matches!(c.desc, ComponentDesc::Canvas(_)))
        .unwrap()
        .handle;
    w.set_component(
        canvas,
        ComponentDesc::Canvas(Canvas {
            scale: CanvasScale::Constant(2.),
            ..Default::default()
        }),
    )
    .unwrap();
    let rect = w
        .published_components()
        .find(|c| c.object == target && matches!(c.desc, ComponentDesc::RectTransform(_)))
        .unwrap()
        .handle;
    w.set_component(
        rect,
        ComponentDesc::RectTransform(RectTransform {
            anchored_position: Vec2::new(50., 50.),
            ..Default::default()
        }),
    )
    .unwrap();
    assert_eq!(
        click(&w, &mut r, 590., 110.).actions,
        vec![UiAction::Activated { object: target }]
    );
    assert!(!click(&w, &mut r, 590., 490.).consumed);
}
#[test]
fn keyboard_traversal_shift_tab_and_repeat_activation() {
    let (mut w, mut r, root, first) = fixture();
    let second = rect(&mut w, Some(root), 200.);
    r.controls.insert(second, UiControl::button());
    let layout = w.ui_layout(UVec2::new(800, 600)).unwrap();
    r.key(&w, &layout, UiKey::Tab, true, false).unwrap();
    assert_eq!(r.focused(), Some(first));
    r.key(&w, &layout, UiKey::Tab, false, false).unwrap();
    r.key(&w, &layout, UiKey::Tab, true, false).unwrap();
    assert_eq!(r.focused(), Some(second));
    r.key(&w, &layout, UiKey::Shift, true, false).unwrap();
    r.key(&w, &layout, UiKey::Tab, false, false).unwrap();
    r.key(&w, &layout, UiKey::Tab, true, false).unwrap();
    assert_eq!(r.focused(), Some(first));
    for key in [UiKey::Enter, UiKey::Space] {
        assert_eq!(
            r.key(&w, &layout, key, true, false).unwrap().actions,
            vec![UiAction::Activated { object: first }]
        );
        assert!(
            r.key(&w, &layout, key, true, true)
                .unwrap()
                .actions
                .is_empty()
        );
        assert!(
            r.key(&w, &layout, key, true, false)
                .unwrap()
                .actions
                .is_empty()
        );
        r.key(&w, &layout, key, false, false).unwrap();
    }
}

#[test]
fn order_within_layer_and_capture_invalidation() {
    let (mut w, mut r, root, back) = fixture();
    let front_root = rect(&mut w, None, 0.);
    w.add_component(
        front_root,
        ComponentDesc::Canvas(Canvas {
            sorting_order: 1,
            ..Default::default()
        }),
    )
    .unwrap();
    let front = rect(&mut w, Some(front_root), 0.);
    r.controls.insert(front, UiControl::button());
    assert_eq!(
        click(&w, &mut r, 400., 300.).actions,
        vec![UiAction::Activated { object: front }]
    );
    send(&w, &mut r, button(true));
    w.set_active(front_root, false).unwrap();
    assert!(send(&w, &mut r, button(false)).actions.is_empty());
    r.modals.insert(root);
    r.controls.get_mut(&back).unwrap().enabled = false;
    let layout = w.ui_layout(UVec2::new(800, 600)).unwrap();
    assert!(
        r.key(&w, &layout, UiKey::Tab, true, false)
            .unwrap()
            .consumed
    );
    assert_eq!(r.focused(), None);
}

#[test]
fn rotated_rectangles_reject_bounding_box_corners_and_stale_controls() {
    let (mut w, mut r, _, target) = fixture();
    w.set_local_transform(
        target,
        worldrunner_world_simulation::LocalTransform {
            rotation: glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!click(&w, &mut r, 465., 235.).consumed);
    assert_eq!(
        click(&w, &mut r, 400., 240.).actions,
        vec![UiAction::Activated { object: target }]
    );
    send(&w, &mut r, button(true));
    w.destroy_object(target).unwrap();
    assert!(send(&w, &mut r, button(false)).actions.is_empty());
    assert!(!click(&w, &mut r, 400., 300.).consumed);
}

#[test]
fn ignored_parent_flags_and_keyboard_focus_respect_modal() {
    let (mut w, mut r, root, first) = fixture();
    w.add_component(
        root,
        ComponentDesc::CanvasGroup(CanvasGroup {
            flags: CanvasGroupFlags {
                interactable: false,
                blocks_raycasts: false,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap();
    ignore_parent_groups(&mut w, first);
    assert_eq!(
        click(&w, &mut r, 400., 300.).actions,
        vec![UiAction::Activated { object: first }]
    );
    let modal = rect(&mut w, Some(root), 200.);
    ignore_parent_groups(&mut w, modal);
    r.controls.insert(modal, UiControl::choice(root, 1));
    r.modals.insert(modal);
    let layout = w.ui_layout(UVec2::new(800, 600)).unwrap();
    assert!(
        r.key(&w, &layout, UiKey::Enter, true, false)
            .unwrap()
            .actions
            .is_empty()
    );
    assert_eq!(r.focused(), None);
    r.key(&w, &layout, UiKey::Tab, true, false).unwrap();
    assert_eq!(r.focused(), Some(modal));
    assert_eq!(
        r.key(&w, &layout, UiKey::Space, true, false)
            .unwrap()
            .actions,
        vec![UiAction::ChoiceSelected {
            group: root,
            index: 1
        }]
    );
    send(&w, &mut r, PointerEvent::Cancel);
    assert_eq!(r.focused(), None);
}
