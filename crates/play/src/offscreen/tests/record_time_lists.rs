//! A culling record admits the renderers that are drawable when the script culls. A render
//! pipeline culls inside `Camera.Render`, while an object that a bake switches on is active. The
//! worlds are synthetic; each test predicts the draws.

use glam::Mat4;
use worldrunner_world_simulation::graphics::RendererListDesc;
use worldrunner_world_simulation::{MeshRenderer, ObjectHandle, World};

use super::lists::{MESH, draw, drawing, lists, meshes, run, scene, scene_with_planes, shaders};
use super::support::{renderer_with, world};

const FORWARD: &str = "UniversalForward";

/// A renderer of mesh `MESH` on layer 1 that draws with the first of the shaders.
fn quad(world: &mut World, active: bool) -> ObjectHandle {
    let [lit, ..] = shaders(world);
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(lit)],
        shadow_casting_mode: 1,
        ..MeshRenderer::default()
    };
    renderer_with(world, (1, active), MESH, renderer)
}

#[test]
fn a_renderer_that_is_active_only_around_the_culling_is_in_the_list_the_culling_makes() {
    let mut world = world();
    let object = quad(&mut world, false);
    world.set_active(object, true).unwrap();
    scene(&mut world, Mat4::IDENTITY, 1 << 1);
    world.set_active(object, false).unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    let drawn = lists(&adapted);
    assert_eq!(drawn.len(), 1);
    assert_eq!(meshes(&adapted, drawn[0]), [(0, 0)]);
    assert_eq!(adapted.report.lists.kept, 1);
}

#[test]
fn a_renderer_that_is_activated_after_the_culling_is_not_in_its_list() {
    let mut world = world();
    let object = quad(&mut world, false);
    scene(&mut world, Mat4::IDENTITY, 1 << 1);
    world.set_active(object, true).unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert!(lists(&adapted).iter().all(|list| list.draws.is_empty()));
    assert_eq!(adapted.report.lists.kept, 0);
}

#[test]
fn a_renderer_that_is_on_a_layer_the_culling_leaves_out_is_not_in_its_list() {
    let mut world = world();
    let object = quad(&mut world, false);
    world.set_active(object, true).unwrap();
    scene(&mut world, Mat4::IDENTITY, 1 << 2);
    world.set_active(object, false).unwrap();
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    assert_eq!(adapted.report.lists.kept, 0);
}

/// A shadow list whose split holds the origin (the volume where y <= 3).
fn shadow_list_over_the_origin() -> RendererListDesc {
    RendererListDesc::Shadow {
        cull: 5,
        light_index: 0,
        planes: vec![[0.0, -1.0, 0.0, 3.0]],
        sphere: [0.0; 4],
        objects_filter: 0,
        projection: 2,
        use_rendering_layer_mask_test: false,
    }
}

/// The camera's volume is x >= 5, which leaves the renderer at the origin out of its view.
fn cull_without_the_origin(world: &mut World) {
    scene_with_planes(world, Mat4::IDENTITY, 1 << 1, vec![[1.0, 0.0, 0.0, -5.0]]);
}

#[test]
fn a_caster_outside_the_view_that_was_drawable_at_the_culling_still_reaches_the_shadow_list() {
    let mut world = world();
    let object = quad(&mut world, false);
    world.set_active(object, true).unwrap();
    cull_without_the_origin(&mut world);
    world.set_active(object, false).unwrap();
    draw(&mut world, shadow_list_over_the_origin());
    let adapted = run(&mut world, |_| {});
    let drawn = lists(&adapted);
    assert_eq!(drawn.len(), 1);
    assert_eq!(meshes(&adapted, drawn[0]), [(0, 0)]);
    assert_eq!(adapted.report.lists.kept, 0);
    assert_eq!(adapted.report.lists.shadow_outside_view, 1);
}

#[test]
fn a_caster_outside_the_view_that_is_activated_after_the_culling_is_not_in_the_shadow_list() {
    let mut world = world();
    let object = quad(&mut world, false);
    cull_without_the_origin(&mut world);
    world.set_active(object, true).unwrap();
    draw(&mut world, shadow_list_over_the_origin());
    let adapted = run(&mut world, |_| {});
    assert!(lists(&adapted).iter().all(|list| list.draws.is_empty()));
    assert_eq!(adapted.report.lists.shadow_outside_view, 0);
}
