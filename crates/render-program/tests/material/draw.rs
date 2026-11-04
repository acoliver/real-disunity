//! Materials drawn into a target: the colour a property gives, the variant a feature selects,
//! and the clip convention of a WGSL position.

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::registry::ProgramDesc;
use worldrunner_render_program::runtime::{Geometry, MaterialDraw, VertexInput};
use worldrunner_render_program::state::RenderState;

use crate::common::{
    COLOR, Gpu, Rig, assert_all, assert_pixel, colored, draw_of, fragment_stage,
    full_target_triangle, rectangle, vertex_stage,
};

#[test]
fn a_full_target_triangle_takes_the_colour_of_the_materials_property() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    let image = rig.render(&draw).unwrap();
    assert_all(&image, COLOR);
}

#[test]
fn a_feature_selects_the_variant_that_doubles_red_and_clearing_it_restores_the_first() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);
    assert_all(&rig.render(&draw).unwrap(), COLOR);

    rig.runtime.set_features(rig.material, &["DOUBLE"]).unwrap();
    assert_all(&rig.render(&draw).unwrap(), [0.5, 0.5, 0.75, 1.0]);

    rig.runtime.set_features(rig.material, &[]).unwrap();
    assert_all(&rig.render(&draw).unwrap(), COLOR);
}

/// A far rectangle over the top half and a near one over the left half. Which rows each covers
/// shows the Y convention, and the overlap shows the depth test: the far one is drawn last.
#[test]
fn an_asymmetric_scene_with_depth_shows_that_wgsl_positions_are_used_as_written() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let near_color = [0.5, 0.25, 0.0, 1.0];
    let near_material = rig
        .runtime
        .create_material(rig.program, colored(near_color))
        .unwrap();
    let near = rectangle(&mut rig.runtime, [-1.0, -1.0, 0.0, 1.0], 0.25);
    rig.render(&draw_of(near_material, near, 6)).unwrap();
    let far = rig.draw([-1.0, 0.0, 1.0, 1.0], 0.75);
    let image = rig.render(&far).unwrap();

    for (column, row, want) in [
        (0, 0, near_color),
        (1, 1, near_color),
        (0, 3, near_color),
        (1, 2, near_color),
        (2, 0, COLOR),
        (3, 1, COLOR),
        (2, 2, [0.0; 4]),
        (3, 3, [0.0; 4]),
    ] {
        assert_pixel(&image, (column, row), want);
    }
}

fn instanced_rig(gpu: &Gpu) -> (Rig, MaterialDraw) {
    let mut rig = Rig::new(gpu);
    let program = rig
        .runtime
        .register_program(ProgramDesc::single(
            "instanced",
            RenderState::opaque_depth(),
            vertex_stage("vs_instanced"),
            fragment_stage("fs"),
        ))
        .unwrap();
    rig.material = rig.runtime.create_material(program, colored(COLOR)).unwrap();
    let column = rectangle(&mut rig.runtime, [-1.0, -1.0, -0.5, 1.0], 0.5);
    let shifts: Vec<u8> = [0.0_f32, 0.5, 1.0, 1.5]
        .iter()
        .flat_map(|x| [*x, 0.0])
        .flat_map(f32::to_le_bytes)
        .collect();
    let shifts = VertexInput {
        buffer: rig.runtime.create_buffer("shifts", &shifts).unwrap(),
        stride: 8,
        step: wgpu::VertexStepMode::Instance,
        attributes: wgpu::vertex_attr_array![1 => Float32x2].to_vec(),
    };
    let draw = MaterialDraw {
        material: rig.material,
        vertices: vec![column, shifts],
        geometry: Geometry::Vertices { first: 0, count: 6 },
        instances: 1..3,
        overrides: worldrunner_render_program::properties::PropertySet::new(),
    };
    (rig, draw)
}

#[test]
fn an_instance_range_draws_exactly_the_instances_in_it() {
    let gpu = Gpu::new();
    let (mut rig, draw) = instanced_rig(&gpu);
    let image = rig.render(&draw).unwrap();
    for row in 0..4 {
        assert_pixel(&image, (0, row), [0.0; 4]);
        assert_pixel(&image, (1, row), COLOR);
        assert_pixel(&image, (2, row), COLOR);
        assert_pixel(&image, (3, row), [0.0; 4]);
    }
}

#[test]
fn an_instance_range_past_its_buffer_or_backwards_is_refused_by_name() {
    let gpu = Gpu::new();
    let (mut rig, mut draw) = instanced_rig(&gpu);
    draw.instances = 0..5;
    assert!(matches!(
        rig.render(&draw).unwrap_err(),
        ProgramError::VertexRangePastBuffer {
            slot: 1,
            needed: 40,
            have: 32
        }
    ));
    #[allow(clippy::reversed_empty_ranges)]
    {
        draw.instances = 3..1;
    }
    assert_eq!(
        rig.render(&draw).unwrap_err(),
        ProgramError::InstanceRange { start: 3, end: 1 }
    );
    draw.instances = 2..2;
    let image = rig.render(&draw).unwrap();
    assert_pixel(&image, (2, 0), [0.0; 4]);
}
