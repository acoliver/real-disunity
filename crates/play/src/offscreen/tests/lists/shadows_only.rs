//! A renderer that only casts shadows is in every shadow list and in no drawing list.

use glam::{Mat4, Vec3};
use worldrunner_world_simulation::graphics::RendererListDesc;

use super::{
    FORWARD, counts, draw, drawing, lists, meshes, placed_casting, run, scene, shaders,
};

const SHADOWS_ONLY: u32 = 3;
const CASTS_AND_DRAWS: u32 = 1;

fn shadow_list() -> RendererListDesc {
    RendererListDesc::Shadow {
        cull: 5,
        light_index: 0,
        planes: Vec::new(),
        sphere: [0.0; 4],
        objects_filter: 0,
        projection: 2,
        use_rendering_layer_mask_test: false,
    }
}

#[test]
fn a_drawing_list_leaves_out_a_renderer_that_casts_shadows_only() {
    let mut world = super::world();
    let [lit, ..] = shaders(&mut world);
    for (number, mode) in [(0, CASTS_AND_DRAWS), (1, SHADOWS_ONLY)] {
        let at = Vec3::new(f32::from(u8::try_from(number).unwrap()), 0.0, 0.0);
        placed_casting(&mut world, (number, 0, at), &[lit], (None, mode));
    }
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("casts shadows only", 1)])
    );
}

#[test]
fn a_shadow_list_keeps_the_renderer_that_casts_shadows_only_next_to_one_that_draws() {
    let mut world = super::world();
    let [lit, ..] = shaders(&mut world);
    for (number, mode) in [(0, CASTS_AND_DRAWS), (1, SHADOWS_ONLY)] {
        let at = Vec3::new(f32::from(u8::try_from(number).unwrap()), 0.0, 0.0);
        placed_casting(&mut world, (number, 0, at), &[lit], (None, mode));
    }
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, shadow_list());
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0), (1, 0)]);
    assert!(adapted.report.lists.left_out.is_empty());
}

#[test]
fn one_cull_feeds_a_drawing_list_without_and_a_shadow_list_with_the_shadows_only_renderer() {
    let mut world = super::world();
    let [lit, ..] = shaders(&mut world);
    for (number, mode) in [(0, CASTS_AND_DRAWS), (1, SHADOWS_ONLY)] {
        let at = Vec3::new(f32::from(u8::try_from(number).unwrap()), 0.0, 0.0);
        placed_casting(&mut world, (number, 0, at), &[lit], (None, mode));
    }
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&[FORWARD]));
    draw(&mut world, shadow_list());
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 2);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(meshes(&adapted, listed[1]), [(0, 0), (1, 0)]);
    assert_eq!(adapted.report.lists.kept, 2);
}
