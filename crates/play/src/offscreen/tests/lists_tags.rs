//! Renderer lists name the passes they draw by the `LightMode` tag, whatever the case its shader spells it in.

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};
use worldrunner_world_simulation::graphics::RendererListDesc;

use super::super::bounds::Bounds;
use super::lists::{MESH, counts, draw, drawing, lists, meshes, placed, run, scene, shaders};
use super::support::{material, shader, world};

fn shadow_split(sphere: [f32; 4]) -> RendererListDesc {
    RendererListDesc::Shadow {
        cull: 5,
        light_index: 0,
        planes: Vec::new(),
        sphere,
        objects_filter: 0,
        projection: 2,
        use_rendering_layer_mask_test: false,
    }
}

#[test]
fn a_shadow_list_draws_a_caster_whose_pass_tag_is_spelled_in_capitals() {
    let mut world = world();
    let caster = shader(&mut world, "forward-and-shouting-caster");
    let upper = material(&mut world, caster);
    placed(&mut world, (0, 0, Vec3::ZERO), &[upper], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, shadow_split([0.0, 0.0, 0.0, 5.0]));
    let adapted = run(&mut world, |available| {
        let cube = Bounds {
            min: [-0.5; 3],
            max: [0.5; 3],
        };
        available.bounds.insert(MESH, cube);
    });
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(adapted.report.lists.shadow_drawn, 1);
    assert_eq!(adapted.report.lists.left_out, BTreeMap::new());
}

#[test]
fn a_drawing_list_names_a_pass_whatever_the_case_of_its_tag() {
    let mut world = world();
    let [lit, unlit, _] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[unlit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, drawing(&["universalforward"]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("no pass with the list's tags", 1)])
    );
}
