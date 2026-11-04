#![cfg(feature = "ui")]
mod ui_font;
use worldrunner_world_simulation::ui::{
    Canvas, FontBook, RectTransform, TextRun, UiLimits, contribution::extract_ui,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, ParentSpace, World, WorldSettings};

#[test]
fn text_composition_uses_hierarchy_instead_of_object_identity() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            canvas,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .add_component(canvas, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    let parent = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(parent, Some(canvas), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    for object in [child, parent] {
        world
            .add_component(
                object,
                ComponentDesc::RectTransform(RectTransform::default()),
            )
            .unwrap();
        world
            .add_component(object, ComponentDesc::Text(TextRun::new(font)))
            .unwrap();
    }
    let plan = extract_ui(&world, &fonts, [64, 64]).unwrap();
    assert_eq!(
        plan.texts.iter().map(|t| t.object).collect::<Vec<_>>(),
        [parent, child]
    );
}

#[test]
fn text_clip_uses_scaled_root_canvas_space_through_nested_canvases() {
    use worldrunner_world_simulation::ui::{CanvasRenderer, CanvasScale};
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    let nested = world.create_object(ObjectDesc::default()).unwrap();
    let text = world.create_object(ObjectDesc::default()).unwrap();
    for object in [root, nested, text] {
        world
            .add_component(
                object,
                ComponentDesc::RectTransform(RectTransform::default()),
            )
            .unwrap();
    }
    world
        .add_component(
            root,
            ComponentDesc::Canvas(Canvas {
                scale: CanvasScale::Constant(2.),
                ..Canvas::default()
            }),
        )
        .unwrap();
    world
        .add_component(nested, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    world
        .set_parent(nested, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(text, Some(nested), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(text, ComponentDesc::Text(TextRun::new(font)))
        .unwrap();
    world
        .add_component(
            text,
            ComponentDesc::CanvasRenderer(CanvasRenderer {
                clip_rect: Some([-8., -8., 8., 8.]),
                ..CanvasRenderer::default()
            }),
        )
        .unwrap();
    let plan = extract_ui(&world, &fonts, [64, 64]).unwrap();
    assert_eq!(plan.texts[0].clip_rect, [16, 16, 16, 16]);
    assert_eq!(
        plan.texts[0].hierarchy_order,
        plan.layout.rect(text).unwrap().draw_order().2
    );
}
