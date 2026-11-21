//! A culling record admits the renderers that are drawable when the script culls. A render
//! pipeline culls inside `Camera.Render`, while an object that a bake switches on is active. The
//! worlds are synthetic; each test predicts the draws.

use glam::Mat4;
use worldrunner_world_simulation::{MeshRenderer, ObjectHandle, World};

use super::lists::{MESH, draw, drawing, lists, meshes, run, scene, shaders};
use super::support::{renderer_with, world};

const FORWARD: &str = "UniversalForward";

/// A renderer of mesh `MESH` on layer 1 that draws with the first of the shaders.
fn quad(world: &mut World, active: bool) -> ObjectHandle {
    let [lit, ..] = shaders(world);
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(lit)],
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
