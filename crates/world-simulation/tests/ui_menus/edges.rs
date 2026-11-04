use super::*;

#[test]
fn stack_cross_alignment_and_inactive_children_are_exact() {
    let (mut world, mut router, root) = fixture();
    let item = rect(&mut world, Some(root), Vec2::new(20., 10.));
    let hidden = rect(&mut world, Some(root), Vec2::splat(50.));
    world.set_active(hidden, false).unwrap();
    for (align, x, width) in [
        (UiAlign::Start, 2., 20.),
        (UiAlign::End, 76., 20.),
        (UiAlign::Stretch, 2., 94.),
    ] {
        router.layout.groups.insert(
            root,
            UiLayoutGroup::Stack {
                axis: UiAxis::Vertical,
                spacing: 3,
                padding: [2, 4, 6, 8],
                align,
            },
        );
        let resolved = layout(&world, &router);
        assert_eq!(bottom(&resolved, item), Vec2::new(x, 84.));
        assert_eq!(resolved.rect(item).unwrap().size, Vec2::new(width, 10.));
        assert!(resolved.rect(hidden).is_none());
    }
}
#[test]
fn fractional_scroll_limits_and_nested_clips_stay_bounded() {
    let (mut world, mut router, root) = fixture();
    let (view, items) = list(&mut world, &mut router, root);
    let handle = world
        .published_components()
        .find(|c| c.object == view && matches!(c.desc, ComponentDesc::RectTransform(_)))
        .unwrap()
        .handle;
    world
        .set_component(
            handle,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: Vec2::new(40., 40.25),
                ..Default::default()
            }),
        )
        .unwrap();
    let resolved = layout(&world, &router);
    router
        .layout
        .scroll_to(&resolved, view, Vec2::splat(500.))
        .unwrap();
    assert!(router.layout.scrolls[&view].offset().y <= 39.75);
    let nested = rect(&mut world, Some(items[0]), Vec2::new(80., 20.));
    let content = rect(&mut world, Some(nested), Vec2::new(80., 60.));
    router
        .layout
        .scrolls
        .insert(nested, UiScrollView::new(content));
    let resolved = layout(&world, &router);
    let [outer_low, outer_high] = resolved.rect(items[0]).unwrap().clip_bounds.unwrap();
    let [low, high] = resolved.rect(content).unwrap().clip_bounds.unwrap();
    assert!(low.cmpge(outer_low).all() && high.cmple(outer_high).all());
}
fn color(world: &World, router: &UiRouter, fonts: &FontBook, item: ObjectHandle) -> [u32; 4] {
    let mut plan = contribution::extract_ui_layout(world, fonts, layout(world, router)).unwrap();
    router.style_plan(world, &mut plan).unwrap();
    plan.texts
        .iter()
        .find(|t| t.object == item)
        .unwrap()
        .color
        .map(f32::to_bits)
}
#[test]
fn authored_styles_cover_normal_focus_selection_hover_press_and_disabled() {
    let (mut world, mut router, root) = fixture();
    let (_, items) = list(&mut world, &mut router, root);
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let style = UiStyle {
        normal: [0., 0., 0., 1.],
        focused: [1., 0., 0., 1.],
        selected: [0., 1., 0., 1.],
        hovered: [0., 0., 1., 1.],
        pressed: [1., 1., 0., 1.],
        disabled: [1., 0., 1., 1.],
    };
    for &item in &items {
        world
            .add_component(item, ComponentDesc::Text(TextRun::new(font)))
            .unwrap();
        router.styles.insert(item, style);
    }
    assert_eq!(
        color(&world, &router, &fonts, items[0]),
        style.normal.map(f32::to_bits)
    );
    key(&world, &mut router, UiKey::Tab);
    assert_eq!(
        color(&world, &router, &fonts, items[0]),
        style.focused.map(f32::to_bits)
    );
    key(&world, &mut router, UiKey::Down);
    key(&world, &mut router, UiKey::Tab);
    assert_eq!(
        color(&world, &router, &fonts, items[1]),
        style.selected.map(f32::to_bits)
    );
    key(&world, &mut router, UiKey::Home);
    key(&world, &mut router, UiKey::Tab);
    pointer(&world, &mut router, PointerEvent::Moved([50., 40.]));
    assert_eq!(
        color(&world, &router, &fonts, items[0]),
        style.hovered.map(f32::to_bits)
    );
    pointer(
        &world,
        &mut router,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed: true,
        },
    );
    assert_eq!(
        color(&world, &router, &fonts, items[0]),
        style.pressed.map(f32::to_bits)
    );
    router.controls.get_mut(&items[0]).unwrap().enabled = false;
    assert_eq!(
        color(&world, &router, &fonts, items[0]),
        style.disabled.map(f32::to_bits)
    );
}
