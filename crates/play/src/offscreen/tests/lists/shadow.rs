//! Shadow renderer lists: which casters a split admits and what it leaves out.

use glam::{Mat4, Vec3};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::RendererListDesc;

use super::super::super::bounds::Bounds;
use super::{
    Available, CASTER, FORWARD, MESH, counts, draw, drawing, lists, meshes, placed, placed_casting,
    run, scene, scene_with_planes, shaders, shouting_caster, world,
};

fn shadow_list(planes: Vec<[f32; 4]>, sphere: [f32; 4]) -> RendererListDesc {
    RendererListDesc::Shadow {
        cull: 5,
        light_index: 0,
        planes,
        sphere,
        objects_filter: 0,
        projection: 2,
        use_rendering_layer_mask_test: false,
    }
}

#[test]
fn a_shadow_list_draws_the_casters_inside_the_split() {
    let mut world = world();
    let [lit, unlit, over] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::new(20.0, 0.0, 0.0)), &[lit], None);
    placed(&mut world, (2, 0, Vec3::new(1.0, 0.0, 0.0)), &[unlit], None);
    // The shader of the override draws forward only and casts no shadow.
    placed(&mut world, (3, 0, Vec3::new(0.0, 1.0, 0.0)), &[over], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(
        &mut world,
        RendererListDesc::Shadow {
            cull: 5,
            light_index: 0,
            planes: Vec::new(),
            sphere: [0.0, 0.0, 0.0, 5.0],
            objects_filter: 0,
            projection: 2,
            use_rendering_layer_mask_test: false,
        },
    );
    let adapted = run(&mut world, |available| {
        let cube = Bounds {
            min: [-0.5; 3],
            max: [0.5; 3],
        };
        for number in 0..4 {
            available.bounds.insert(MESH + number, cube);
        }
    });
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(
        listed[0].draws[0].pass,
        PassSelector::Tag {
            key: "LightMode".to_owned(),
            values: vec![CASTER.to_owned()],
        }
    );
    let report = &adapted.report.lists;
    assert_eq!(report.shadow_drawn, 1);
    assert_eq!(report.draws_by_tag, counts(&[(CASTER, 1)]));
    assert_eq!(
        report.left_out,
        counts(&[
            ("no shadow caster pass", 2),
            ("outside the split's sphere", 1)
        ])
    );
}

#[test]
fn a_shadow_list_leaves_out_the_renderers_that_cast_no_shadow() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    for (number, mode) in [(0, 0), (1, 1), (2, 2), (3, 3)] {
        let place = (
            number,
            0,
            Vec3::new(f32::from(u8::try_from(number).unwrap()), 0.0, 0.0),
        );
        placed_casting(&mut world, place, &[lit], (None, mode));
    }
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, shadow_list(Vec::new(), [0.0; 4]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(1, 0), (2, 0), (3, 0)]);
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("casts no shadow", 1)])
    );
}

#[test]
fn a_split_with_planes_keeps_the_casters_toward_the_light_outside_its_sphere() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::new(0.0, 30.0, 0.0)), &[lit], None);
    placed(&mut world, (2, 0, Vec3::new(40.0, 0.0, 0.0)), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    // The sphere wraps the receivers; the planes bound the casters: a column |x| <= 10 open
    // toward +y, where the light is.
    let planes = vec![[1.0, 0.0, 0.0, 10.0], [-1.0, 0.0, 0.0, 10.0]];
    draw(&mut world, shadow_list(planes, [0.0, 0.0, 0.0, 5.0]));
    let adapted = run(&mut world, |available| {
        let cube = Bounds {
            min: [-0.5; 3],
            max: [0.5; 3],
        };
        for number in 0..3 {
            available.bounds.insert(MESH + number, cube);
        }
    });
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0), (1, 0)]);
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("outside the split's planes", 1)])
    );
}

#[test]
fn a_shadow_split_leaves_out_the_casters_wholly_outside_its_planes() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (1, 0, Vec3::new(0.0, 6.0, 0.0)), &[lit], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(
        &mut world,
        RendererListDesc::Shadow {
            cull: 5,
            light_index: 0,
            // Inside where y <= 3: a normal along negative y and the distance 3.
            planes: vec![[0.0, -1.0, 0.0, 3.0]],
            sphere: [0.0; 4],
            objects_filter: 0,
            projection: 1,
            use_rendering_layer_mask_test: false,
        },
    );
    let adapted = run(&mut world, |available| {
        let cube = Bounds {
            min: [-0.5; 3],
            max: [0.5; 3],
        };
        for number in 0..2 {
            available.bounds.insert(MESH + number, cube);
        }
    });
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(
        adapted.report.lists.left_out,
        counts(&[("outside the split's planes", 1)])
    );
}

#[test]
fn a_shadow_caster_pass_is_found_whatever_the_case_of_its_light_mode() {
    let mut world = world();
    let shouting = shouting_caster(&mut world);
    placed(&mut world, (0, 0, Vec3::ZERO), &[shouting], None);
    scene(&mut world, Mat4::IDENTITY, u32::MAX);
    draw(&mut world, shadow_list(Vec::new(), [0.0; 4]));
    let adapted = run(&mut world, |_| {});
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
    assert_eq!(adapted.report.lists.shadow_drawn, 1);
    assert!(adapted.report.lists.left_out.is_empty());
}

/// A scene whose culling result 5 keeps the renderers of `mask` with x >= 5, as a camera looking
/// toward +x from there would, and unit cubes: a ground at x = 10, a caster at the origin
/// that the camera does not see and one at y = 6 that the camera does not see either.
fn camera_looking_down_x(mask: u32) -> World {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::new(10.0, 0.0, 0.0)), &[lit], None);
    placed(&mut world, (1, 0, Vec3::ZERO), &[lit], None);
    placed(&mut world, (2, 0, Vec3::new(0.0, 6.0, 0.0)), &[lit], None);
    // The camera's volume x >= 5: a unit normal along x and the distance -5.
    scene_with_planes(
        &mut world,
        Mat4::IDENTITY,
        mask,
        vec![[1.0, 0.0, 0.0, -5.0]],
    );
    world
}

fn unit_cubes(available: &mut Available) {
    let cube = Bounds {
        min: [-0.5; 3],
        max: [0.5; 3],
    };
    for number in 0..4 {
        available.bounds.insert(MESH + number, cube);
    }
}

#[test]
fn a_shadow_list_draws_a_caster_the_camera_does_not_see_when_the_split_holds_it() {
    let mut world = camera_looking_down_x(u32::MAX);
    // The split's volume is where y <= 3, which holds the ground and the caster at the origin.
    draw(
        &mut world,
        shadow_list(vec![[0.0, -1.0, 0.0, 3.0]], [0.0; 4]),
    );
    let adapted = run(&mut world, unit_cubes);
    let listed = lists(&adapted);
    assert_eq!(listed.len(), 1);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0), (1, 0)]);
    let report = &adapted.report.lists;
    assert_eq!(report.kept, 1);
    assert_eq!(report.out_by_planes, 2);
    assert_eq!(report.shadow_outside_view, 1);
    assert_eq!(
        report.left_out,
        counts(&[("outside the split's planes", 1)])
    );
}

#[test]
fn a_drawing_list_of_the_same_culling_result_still_leaves_out_what_the_camera_does_not_see() {
    let mut world = camera_looking_down_x(u32::MAX);
    draw(&mut world, drawing(&[FORWARD]));
    let adapted = run(&mut world, unit_cubes);
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
}

#[test]
fn a_shadow_list_still_leaves_out_the_casters_the_culling_mask_does_not_keep() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    placed(&mut world, (0, 0, Vec3::new(10.0, 0.0, 0.0)), &[lit], None);
    placed(&mut world, (1, 2, Vec3::ZERO), &[lit], None);
    scene_with_planes(&mut world, Mat4::IDENTITY, 1, vec![[1.0, 0.0, 0.0, -5.0]]);
    draw(
        &mut world,
        shadow_list(vec![[0.0, -1.0, 0.0, 3.0]], [0.0; 4]),
    );
    let adapted = run(&mut world, unit_cubes);
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0)]);
}

#[test]
fn a_shadow_list_leaves_out_an_off_caster_outside_the_view_and_draws_the_others() {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    // The ground the camera sees, then three renderers it does not see, all inside the split:
    // one that casts no shadow (mode 0), a two-sided one (2) and one that only shadows (3).
    let casters = [
        (0, Vec3::new(10.0, 0.0, 0.0), 1),
        (1, Vec3::ZERO, 0),
        (2, Vec3::new(0.0, -1.0, 0.0), 2),
        (3, Vec3::new(0.0, 1.0, 0.0), 3),
    ];
    for (number, position, mode) in casters {
        placed_casting(&mut world, (number, 0, position), &[lit], (None, mode));
    }
    scene_with_planes(
        &mut world,
        Mat4::IDENTITY,
        u32::MAX,
        vec![[1.0, 0.0, 0.0, -5.0]],
    );
    draw(
        &mut world,
        shadow_list(vec![[0.0, -1.0, 0.0, 3.0]], [0.0; 4]),
    );
    let adapted = run(&mut world, unit_cubes);
    let listed = lists(&adapted);
    assert_eq!(meshes(&adapted, listed[0]), [(0, 0), (2, 0), (3, 0)]);
    let report = &adapted.report.lists;
    assert_eq!(report.shadow_outside_view, 2);
    assert_eq!(report.left_out, counts(&[("casts no shadow", 1)]));
}
