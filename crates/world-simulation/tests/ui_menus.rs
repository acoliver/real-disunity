#![cfg(feature = "ui")]
#[path = "ui_menus/edges.rs"]
mod edges;
mod ui_font;
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, PointerButton, PointerEvent,
    PointerWheel, World, WorldSettings,
};

fn rect(w: &mut World, parent: Option<ObjectHandle>, size: Vec2) -> ObjectHandle {
    let object = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        object,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: size,
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    object
}
fn fixture() -> (World, UiRouter, ObjectHandle) {
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut w, None, Vec2::ZERO);
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    (w, UiRouter::default(), root)
}
fn layout(w: &World, r: &UiRouter) -> UiLayout {
    w.ui_layout_with(UVec2::new(100, 100), &r.layout).unwrap()
}
fn key(w: &World, r: &mut UiRouter, key: UiKey) -> UiResponse {
    let l = layout(w, r);
    r.key(w, &l, key, false, false).unwrap();
    r.key(w, &l, key, true, false).unwrap()
}
fn pointer(w: &World, r: &mut UiRouter, event: PointerEvent) -> UiResponse {
    r.pointer(w, &layout(w, r), event).unwrap()
}
fn click(w: &World, r: &mut UiRouter, at: [f64; 2]) -> UiResponse {
    pointer(w, r, PointerEvent::Moved(at));
    pointer(
        w,
        r,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true,
        },
    );
    pointer(
        w,
        r,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: false,
        },
    )
}
fn stack(axis: UiAxis) -> UiLayoutGroup {
    UiLayoutGroup::Stack {
        axis,
        spacing: 3,
        padding: [2, 4, 6, 8],
        align: UiAlign::Center,
    }
}
fn bottom(l: &UiLayout, o: ObjectHandle) -> Vec2 {
    l.rect(o).unwrap().screen_corners.unwrap()[0]
}
#[test]
fn stacks_use_padding_spacing_alignment_and_content_sized_groups() {
    let (mut world, mut router, root) = fixture();
    let fixed = rect(&mut world, Some(root), Vec2::new(20., 10.));
    let group = rect(&mut world, Some(root), Vec2::ZERO);
    let child = rect(&mut world, Some(group), Vec2::new(12., 5.));
    router.layout.groups.insert(root, stack(UiAxis::Vertical));
    router
        .layout
        .groups
        .insert(group, stack(UiAxis::Horizontal));
    router.layout.sizes.insert(group, UiSize::Content);
    let resolved = layout(&world, &router);
    assert_eq!(resolved.rect(group).unwrap().size, Vec2::new(18., 19.));
    assert_eq!(bottom(&resolved, fixed), Vec2::new(39., 84.));
    assert_eq!(bottom(&resolved, group), Vec2::new(40., 62.));
    assert_eq!(bottom(&resolved, child), Vec2::new(42., 70.));
    assert_eq!(resolved, layout(&world, &router));
    router.layout.groups.insert(root, stack(UiAxis::Horizontal));
    router
        .layout
        .sizes
        .insert(fixed, UiSize::Fixed(UVec2::new(30, 12)));
    let resolved = layout(&world, &router);
    assert_eq!(bottom(&resolved, fixed), Vec2::new(2., 45.));
    assert_eq!(bottom(&resolved, group), Vec2::new(35., 42.));
}
#[test]
fn grid_fills_columns_or_rows_in_hierarchy_order() {
    let (mut w, mut r, root) = fixture();
    let children: Vec<_> = (0..5)
        .map(|_| rect(&mut w, Some(root), Vec2::ONE))
        .collect();
    for (flow, expected) in [
        (
            UiGridFlow::Columns(2),
            [Vec2::new(1., 86.), Vec2::new(23., 86.), Vec2::new(1., 73.)],
        ),
        (
            UiGridFlow::Rows(2),
            [Vec2::new(1., 86.), Vec2::new(1., 73.), Vec2::new(23., 86.)],
        ),
    ] {
        r.layout.groups.insert(
            root,
            UiLayoutGroup::Grid {
                flow,
                cell: UVec2::new(20, 10),
                spacing: UVec2::new(2, 3),
                padding: [1, 2, 4, 5],
            },
        );
        let l = layout(&w, &r);
        for (o, at) in children.iter().zip(expected) {
            assert_eq!(bottom(&l, *o), at);
        }
        assert_eq!(l.rect(children[4]).unwrap().size, Vec2::new(20., 10.));
    }
}
fn list(w: &mut World, r: &mut UiRouter, root: ObjectHandle) -> (ObjectHandle, Vec<ObjectHandle>) {
    let viewport = rect(w, Some(root), Vec2::new(40., 40.));
    let content = rect(w, Some(viewport), Vec2::new(40., 80.));
    let items: Vec<_> = (0..4)
        .map(|index| {
            let o = rect(w, Some(content), Vec2::new(40., 20.));
            r.controls.insert(o, UiControl::list_item(content, index));
            o
        })
        .collect();
    r.layout.groups.insert(
        content,
        UiLayoutGroup::Stack {
            axis: UiAxis::Vertical,
            spacing: 0,
            padding: [0; 4],
            align: UiAlign::Start,
        },
    );
    r.layout
        .scrolls
        .insert(viewport, UiScrollView::new(content));
    (viewport, items)
}
#[test]
fn keyboard_and_pointer_select_the_same_list_items_and_reveal_focus() {
    let (mut w, mut r, root) = fixture();
    let (view, items) = list(&mut w, &mut r, root);
    let list = w.parent(items[0]).unwrap().unwrap();
    key(&w, &mut r, UiKey::Tab);
    for (k, index) in [
        (UiKey::Down, 1),
        (UiKey::End, 3),
        (UiKey::Home, 0),
        (UiKey::Down, 1),
        (UiKey::Up, 0),
    ] {
        assert_eq!(
            key(&w, &mut r, k).actions,
            [UiAction::Selected { list, index }]
        );
        assert_eq!(r.focused(), Some(items[index]));
        assert_eq!(r.selected(list), Some(index));
    }
    assert_eq!(
        click(&w, &mut r, [50., 60.]).actions,
        [UiAction::Selected { list, index: 1 }]
    );
    key(&w, &mut r, UiKey::End);
    assert_eq!(r.layout.scrolls[&view].offset(), Vec2::new(0., 40.));
    let l = layout(&w, &r);
    assert_eq!(bottom(&l, items[3]), Vec2::new(30., 30.));
}
#[test]
fn wheel_clamps_offsets_and_clipped_children_never_receive_hits() {
    let (mut w, mut r, root) = fixture();
    let (view, items) = list(&mut w, &mut r, root);
    assert!(!pointer(&w, &mut r, PointerEvent::Moved([50., 80.])).consumed);
    assert!(click(&w, &mut r, [50., 80.]).actions.is_empty());
    pointer(&w, &mut r, PointerEvent::Moved([50., 40.]));
    assert!(
        pointer(
            &w,
            &mut r,
            PointerEvent::Wheel(PointerWheel::Pixels([0., -500.]))
        )
        .consumed
    );
    assert_eq!(r.layout.scrolls[&view].offset(), Vec2::new(0., 40.));
    pointer(
        &w,
        &mut r,
        PointerEvent::Wheel(PointerWheel::Lines([0., 500.])),
    );
    assert_eq!(r.layout.scrolls[&view].offset(), Vec2::ZERO);
    r.controls.get_mut(&items[0]).unwrap().enabled = false;
    assert!(click(&w, &mut r, [50., 40.]).actions.is_empty());
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(items[1]));
    key(&w, &mut r, UiKey::Home);
    assert_eq!(r.focused(), Some(items[1]));
}
fn item_renderer() -> CanvasRenderer {
    CanvasRenderer {
        geometry: Some(std::sync::Arc::new(
            UiGeometry::new(
                [
                    [-20., -10., 0.],
                    [20., -10., 0.],
                    [20., 10., 0.],
                    [-20., 10., 0.],
                ]
                .map(|position| UiVertex {
                    position,
                    uv: [0.; 2],
                    color: [1.; 4],
                })
                .to_vec(),
                vec![0, 1, 2, 0, 2, 3],
            )
            .unwrap(),
        )),
        ..Default::default()
    }
}
#[test]
fn scroll_clip_reaches_text_and_graphic_contributions_and_style() {
    let (mut w, mut r, root) = fixture();
    let (_, items) = list(&mut w, &mut r, root);
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    for &item in &items {
        w.add_component(
            item,
            ComponentDesc::Text(TextRun {
                text: "A".into(),
                ..TextRun::new(font)
            }),
        )
        .unwrap();
        w.add_component(item, ComponentDesc::CanvasRenderer(item_renderer()))
            .unwrap();
    }
    let mut plan = contribution::extract_ui_layout(&w, &fonts, layout(&w, &r)).unwrap();
    assert_eq!(
        plan.texts.iter().map(|t| t.clip_rect).collect::<Vec<_>>(),
        [
            [30, 50, 40, 20],
            [30, 30, 40, 20],
            [30, 30, 40, 0],
            [30, 30, 40, 0]
        ]
    );
    assert!(plan.texts.iter().all(|t| !t.quads.is_empty()));
    assert!(
        plan.graphics
            .iter()
            .all(|g| g.clip_rect == [30, 30, 40, 40])
    );
    key(&w, &mut r, UiKey::Tab);
    r.styles.insert(
        items[0],
        UiStyle {
            focused: [1., 0., 0., 1.],
            ..UiStyle::default()
        },
    );
    r.style_plan(&w, &mut plan).unwrap();
    assert_eq!(
        plan.graphics[0].graphic.quads[0].color.map(f32::to_bits),
        [1., 0., 0., 1.].map(f32::to_bits)
    );
    assert_eq!(
        plan.texts[0].color.map(f32::to_bits),
        [1., 0., 0., 1.].map(f32::to_bits)
    );
}
#[test]
fn nested_modal_close_restores_each_opener_and_skips_disabled_focus() {
    let (mut w, mut r, root) = fixture();
    let opener = rect(&mut w, Some(root), Vec2::new(20., 20.));
    let modal = rect(&mut w, Some(root), Vec2::new(20., 20.));
    let inner = rect(&mut w, Some(modal), Vec2::new(20., 20.));
    for o in [opener, modal, inner] {
        r.controls.insert(o, UiControl::button());
    }
    key(&w, &mut r, UiKey::Tab);
    r.modals.insert(modal);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(modal));
    r.modals.insert(inner);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(inner));
    r.modals.remove(&inner);
    key(&w, &mut r, UiKey::Shift);
    assert_eq!(r.focused(), Some(modal));
    r.modals.remove(&modal);
    key(&w, &mut r, UiKey::Shift);
    assert_eq!(r.focused(), Some(opener));
    r.controls.get_mut(&opener).unwrap().enabled = false;
    key(&w, &mut r, UiKey::Space);
    assert_eq!(r.focused(), None);
}
