//! Shadow renderer lists: which casters a split admits and what it leaves out.

use glam::{Mat4, Vec3};
use worldrunner_render_program::unity::shader::PassSelector;
use worldrunner_world_simulation::graphics::RendererListDesc;

use super::super::super::bounds::Bounds;
use super::{
    CASTER, MESH, counts, draw, lists, meshes, placed, placed_casting, run, scene, shaders, world,
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
