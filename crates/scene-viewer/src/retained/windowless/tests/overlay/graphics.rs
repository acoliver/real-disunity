mod clipping;
mod ordering;
mod scrolled;
use super::*;
use worldrunner_world_simulation::ui::{
    Canvas, CanvasRenderer, FontBook, RectTransform, UiGeometry, UiLimits, UiVertex,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectHandle, ParentSpace};

pub(super) fn graphic_world() -> (World, ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
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
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(object, Some(canvas), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    let vertices = [[-24., -24.], [-8., -24.], [-8., -8.], [-24., -8.]]
        .into_iter()
        .zip([[0., 0.], [1., 0.], [1., 1.], [0., 1.]])
        .map(|([x, y], uv)| UiVertex {
            position: [x, y, 0.],
            uv,
            color: [1.; 4],
        })
        .collect();
    let renderer = CanvasRenderer {
        color: [1., 0., 0., 1.],
        geometry: Some(std::sync::Arc::new(
            UiGeometry::new(vertices, vec![0, 1, 2, 0, 2, 3]).unwrap(),
        )),
        ..CanvasRenderer::default()
    };
    world
        .add_component(object, ComponentDesc::CanvasRenderer(renderer))
        .unwrap();
    (world, object)
}

pub(super) fn extracted(world: &World) -> UiRenderPlan {
    worldrunner_render_plan::ui::extract_ui(
        world,
        &FontBook::new(UiLimits::default()).unwrap(),
        [64, 64],
    )
    .unwrap()
}

#[test]
fn opaque_graphic_is_drawn_on_the_shadowed_retained_frame() {
    let (world, _) = graphic_world();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    // Present generation 0 before the UI update, so its frame reports the scene's initial uploads.
    capture(&mut run, &events);
    apply(&mut run, extracted(&world));
    region(&capture(&mut run, &events).1, [255, 0, 0, 255]);
    assert_statistics(&run, 1, 1);
    run.shutdown().unwrap();
}
