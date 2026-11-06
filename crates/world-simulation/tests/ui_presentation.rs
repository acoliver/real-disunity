#![cfg(feature = "ui")]
mod ui_font;

use std::sync::Arc;
use worldrunner_world_simulation::ui::{
    Canvas, CanvasMode, CanvasRenderer, FontBook, RectTransform, TextRun, UiGeometry, UiLimits,
    UiVertex,
    contribution::extract_ui,
    presentation::{UiDrawKind, UiPresentation},
};
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, World, WorldSettings,
};

fn rectangle(world: &mut World, parent: Option<ObjectHandle>) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    object
}

fn canvas(world: &mut World, order: i32, mode: CanvasMode) -> ObjectHandle {
    let object = rectangle(world, None);
    world
        .add_component(
            object,
            ComponentDesc::Canvas(Canvas {
                sorting_order: order,
                mode,
                ..Canvas::default()
            }),
        )
        .unwrap();
    object
}

fn graphic(world: &mut World, object: ObjectHandle, clip: Option<[f32; 4]>) {
    let vertices = [[-16., -16.], [16., -16.], [16., 16.], [-16., 16.]].map(|[x, y]| UiVertex {
        position: [x, y, 0.],
        uv: [0., 0.],
        color: [1.; 4],
    });
    let geometry = UiGeometry::new(vertices.to_vec(), vec![0, 1, 2, 0, 2, 3]).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::CanvasRenderer(CanvasRenderer {
                geometry: Some(Arc::new(geometry)),
                clip_rect: clip,
                ..CanvasRenderer::default()
            }),
        )
        .unwrap();
}

#[test]
fn overlay_export_interleaves_canvas_and_sibling_order_before_draw_kind() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let foreground = canvas(&mut world, 7, CanvasMode::ScreenSpaceOverlay);
    let back = canvas(&mut world, -3, CanvasMode::ScreenSpaceOverlay);
    let first = rectangle(&mut world, Some(back));
    let second = rectangle(&mut world, Some(back));
    for object in [second, first, foreground] {
        graphic(&mut world, object, None);
    }
    for object in [first, foreground] {
        let mut text = TextRun::new(font);
        text.text = "A".into();
        world
            .add_component(object, ComponentDesc::Text(text))
            .unwrap();
    }
    let plan = extract_ui(&world, &fonts, [64, 64]).unwrap();
    let snapshot = UiPresentation::from_plan(&plan);
    let expected = [
        (first, UiDrawKind::Graphic),
        (first, UiDrawKind::Text),
        (second, UiDrawKind::Graphic),
        (foreground, UiDrawKind::Graphic),
        (foreground, UiDrawKind::Text),
    ];
    assert_eq!(snapshot.draws.len(), expected.len());
    for (draw, (object, kind)) in snapshot.draws.iter().zip(expected) {
        assert_eq!(
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order),
            plan.layout.rect(object).unwrap().draw_order()
        );
        assert_eq!(draw.kind, kind);
        assert_eq!(draw.clip_rect, [0, 0, 64, 64]);
    }
}

#[test]
fn overlay_export_keeps_child_clip_and_leaves_world_canvas_in_the_scene() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let screen = canvas(&mut world, 0, CanvasMode::ScreenSpaceOverlay);
    let child = rectangle(&mut world, Some(screen));
    graphic(&mut world, child, Some([-8., -8., 8., 8.]));
    let spatial = canvas(&mut world, 99, CanvasMode::WorldSpace);
    graphic(&mut world, spatial, None);
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_ui(&world, &fonts, [64, 64]).unwrap();
    assert!(plan.graphics.iter().any(|g| !g.graphic.overlay));
    let snapshot = UiPresentation::from_plan(&plan);
    assert_eq!(snapshot.draws.len(), 1);
    assert_eq!(snapshot.draws[0].clip_rect, [24, 24, 8, 8]);
    assert_eq!(snapshot.draws[0].sorting_order, 0);
}
