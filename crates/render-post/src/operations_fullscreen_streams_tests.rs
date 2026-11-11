//! A fullscreen operation may read vertex streams, and the streams are checked like those of a
//! draw packet.
use super::operations_draw_test_support::{BLACK, GREEN, Rig, pixel, quad, reads};
use super::operations_tests::{buffer, id, underlying};
use crate::contracts::*;

fn blit(rig: &mut Rig, stream: &BufferView, count: u32) -> TypedOperation {
    let layout = rig
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor::default());
    let pipeline = rig
        .device
        .create_render_pipeline(&rig.pipeline("fs_plain", Some(&layout)));
    TypedOperation::new(
        id("blit"),
        TypedWork::Fullscreen {
            pass: rig.pass("blit"),
            pipeline,
            vertices: Some(FullscreenVertices {
                streams: vec![VertexStream {
                    view: stream.clone(),
                    stride: 8,
                    extent: 8,
                }],
                count,
            }),
        },
    )
}

fn refusal(rig: &Rig, operation: TypedOperation) -> ContractError {
    underlying(
        rig.registry
            .compile_operations(&rig.device, &[operation])
            .err()
            .unwrap(),
    )
}

#[test]
fn a_fullscreen_operation_draws_the_count_of_vertices_it_names() {
    let mut rig = Rig::new();
    let stream = rig.vertices(&quad([0.0, 1.0]));
    let mut operation = blit(&mut rig, &stream, 6);
    operation.buffers = reads(&[], &[&stream]);
    let image = rig.run(&[operation]);
    for row in 0..4 {
        assert_eq!(pixel(&image, 1, row), BLACK);
        assert_eq!(pixel(&image, 2, row), GREEN);
        assert_eq!(pixel(&image, 3, row), GREEN);
    }
}

#[test]
fn only_the_first_triangle_is_drawn_when_the_count_is_three() {
    let mut rig = Rig::new();
    let stream = rig.vertices(&quad([0.0, 1.0]));
    let mut operation = blit(&mut rig, &stream, 3);
    operation.buffers = reads(&[], &[&stream]);
    let image = rig.run(&[operation]);
    // The first triangle covers the lower right half of the quad's columns.
    assert_eq!(pixel(&image, 3, 3), GREEN);
    assert_eq!(pixel(&image, 3, 0), BLACK);
}

#[test]
fn streams_past_their_buffer_or_undeclared_or_not_vertex_buffers_are_named_errors() {
    let mut rig = Rig::new();
    let stream = rig.vertices(&quad([0.0, 1.0]));

    let mut long = blit(&mut rig, &stream, 7);
    long.buffers = reads(&[], &[&stream]);
    assert!(matches!(refusal(&rig, long), ContractError::Range(_)));

    let mut empty = blit(&mut rig, &stream, 0);
    empty.buffers = reads(&[], &[&stream]);
    assert_eq!(refusal(&rig, empty), ContractError::Pipeline);

    let undeclared = blit(&mut rig, &stream, 6);
    assert!(matches!(
        refusal(&rig, undeclared),
        ContractError::Binding(_)
    ));

    let plain = buffer(&rig.device, 48, wgpu::BufferUsages::COPY_DST);
    let not_vertex = rig.import(&plain);
    let mut wrong = blit(&mut rig, &not_vertex, 6);
    wrong.buffers = reads(&[], &[&not_vertex]);
    assert!(matches!(refusal(&rig, wrong), ContractError::Binding(_)));
}
