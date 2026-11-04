use super::*;
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::{
    UiAlign, UiAxis, UiControl, UiLayoutGroup, UiRouter, UiScrollView,
    contribution::extract_ui_layout,
};

fn rectangle(world: &mut World, parent: ObjectHandle, size: Vec2) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(object, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: size,
                ..Default::default()
            }),
        )
        .unwrap();
    object
}
fn item_renderer(color: [f32; 4]) -> CanvasRenderer {
    CanvasRenderer {
        color,
        geometry: Some(std::sync::Arc::new(
            UiGeometry::new(
                [
                    [-16., -8., 0.],
                    [16., -8., 0.],
                    [16., 8., 0.],
                    [-16., 8., 0.],
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
fn scrolled_plan() -> UiRenderPlan {
    let (mut world, original) = graphic_world();
    let root = world.parent(original).unwrap().unwrap();
    world.set_active(original, false).unwrap();
    let view = rectangle(&mut world, root, Vec2::new(32., 16.));
    let content = rectangle(&mut world, view, Vec2::new(32., 48.));
    let mut router = UiRouter::default();
    for (index, color) in [[1., 0., 0., 1.], [0., 1., 0., 1.], [0., 0., 1., 1.]]
        .into_iter()
        .enumerate()
    {
        let item = rectangle(&mut world, content, Vec2::new(32., 16.));
        router
            .controls
            .insert(item, UiControl::list_item(content, index));
        world
            .add_component(item, ComponentDesc::CanvasRenderer(item_renderer(color)))
            .unwrap();
    }
    router.layout.groups.insert(
        content,
        UiLayoutGroup::Stack {
            axis: UiAxis::Vertical,
            spacing: 0,
            padding: [0; 4],
            align: UiAlign::Start,
        },
    );
    router
        .layout
        .scrolls
        .insert(view, UiScrollView::new(content));
    let layout = world
        .ui_layout_with(UVec2::splat(64), &router.layout)
        .unwrap();
    router
        .layout
        .scroll_to(&layout, view, Vec2::new(0., 8.))
        .unwrap();
    let layout = world
        .ui_layout_with(UVec2::splat(64), &router.layout)
        .unwrap();
    extract_ui_layout(&world, &FontBook::new(UiLimits::default()).unwrap(), layout).unwrap()
}
#[test]
fn scrolled_selection_list_draws_exact_regions_and_preserves_pixels_outside_viewport() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    apply(&mut run, scrolled_plan());
    let image = capture(&mut run, &events).1;
    for y in 0..64 {
        for x in 0..64 {
            let expected = match (x, y) {
                (16..48, 24..32) => &[0, 255, 0, 255],
                (16..48, 32..40) => &[255, 0, 0, 255],
                _ => pixel(&baseline, x, y),
            };
            assert_eq!(pixel(&image, x, y), expected, "pixel {x},{y}");
        }
    }
    run.shutdown().unwrap();
}
