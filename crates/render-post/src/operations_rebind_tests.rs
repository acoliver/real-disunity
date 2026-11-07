//! A checked draw can be rebound to other bindings and geometry without creating its pipeline
//! again, and only to bindings and geometry the pipeline was made for.
use super::operations_draw_support::{BLACK, BLUE, RED, Rig, geometry, pixel, quad, reads};
use super::operations_tests::{id, underlying};
use crate::contracts::*;

fn two_halves(rig: &mut Rig) -> (Vec<TypedOperation>, CheckedDraw) {
    let left = rig.vertices(&quad([-1.0, 0.0]));
    let right = rig.vertices(&quad([0.0, 1.0]));
    let red = rig.fill([1.0, 0.0, 0.0, 1.0], wgpu::ShaderStages::FRAGMENT);
    let blue = rig.fill([0.0, 0.0, 1.0, 1.0], wgpu::ShaderStages::FRAGMENT);
    let first = rig
        .draw("fs", vec![red.bindings.clone()], geometry(left.clone()))
        .unwrap();
    let second = first
        .rebind(vec![blue.bindings.clone()], geometry(right.clone()))
        .unwrap();
    let mut operation = TypedOperation::new(
        id("rebind"),
        TypedWork::Draws {
            pass: rig.pass("rebind"),
            draws: vec![first.clone(), second],
        },
    );
    operation.buffers = reads(&[&red.view, &blue.view], &[&left, &right]);
    (vec![operation], first)
}

#[test]
fn a_rebound_draw_draws_its_own_bindings_and_geometry() {
    let mut rig = Rig::new();
    let (operations, _) = two_halves(&mut rig);
    let image = rig.run(&operations);
    for row in 0..4 {
        assert_eq!(pixel(&image, 0, row), RED);
        assert_eq!(pixel(&image, 1, row), RED);
        assert_eq!(pixel(&image, 2, row), BLUE);
        assert_eq!(pixel(&image, 3, row), BLUE);
    }
}

#[test]
fn the_draw_that_was_rebound_keeps_its_own_bindings_and_geometry() {
    let mut rig = Rig::new();
    let (operations, first) = two_halves(&mut rig);
    let TypedWork::Draws { pass, .. } = &operations[0].work else {
        unreachable!()
    };
    let alone = TypedOperation {
        work: TypedWork::Draws {
            pass: pass.clone(),
            draws: vec![first],
        },
        ..operations[0].clone()
    };
    let image = rig.run(&[alone]);
    assert_eq!(pixel(&image, 1, 2), RED);
    assert_eq!(pixel(&image, 2, 2), BLACK, "the right half was not drawn");
}

#[test]
fn rebinding_refuses_bindings_and_geometry_the_pipeline_was_not_made_for() {
    let mut rig = Rig::new();
    let stream = rig.vertices(&quad([-1.0, 0.0]));
    let fill = rig.fill([1.0; 4], wgpu::ShaderStages::FRAGMENT);
    let draw = rig
        .draw("fs", vec![fill.bindings.clone()], geometry(stream.clone()))
        .unwrap();

    let wider = rig.fill(
        [1.0; 4],
        wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
    );
    assert_eq!(
        draw.rebind(vec![wider.bindings], geometry(stream.clone()))
            .err(),
        Some(ContractError::Pipeline),
        "another visibility is another layout"
    );
    assert_eq!(
        draw.rebind(Vec::new(), geometry(stream.clone())).err(),
        Some(ContractError::Pipeline),
        "no groups where the pipeline has one"
    );
    let mut two_streams = geometry(stream.clone());
    two_streams.vertex_buffers.push(stream.clone());
    assert_eq!(
        draw.rebind(vec![fill.bindings.clone()], two_streams).err(),
        Some(ContractError::Pipeline),
        "the pipeline reads one stream"
    );
    let mut offset = geometry(stream.clone());
    offset.base_vertex = 1;
    assert_eq!(
        draw.rebind(vec![fill.bindings.clone()], offset).err(),
        Some(ContractError::Pipeline),
        "a base vertex needs an index buffer"
    );
    let mut long = geometry(stream);
    long.elements = 0..7;
    assert!(matches!(
        draw.rebind(vec![fill.bindings], long).err(),
        Some(ContractError::Range(_))
    ));
}

#[test]
fn a_rebound_draw_is_checked_like_any_other_when_the_list_is_compiled() {
    let mut rig = Rig::new();
    let (mut operations, _) = two_halves(&mut rig);
    operations[0].buffers.clear();
    assert!(matches!(
        underlying(
            rig.registry
                .compile_operations(&rig.device, &operations)
                .err()
                .unwrap()
        ),
        ContractError::Binding(_)
    ));
}
