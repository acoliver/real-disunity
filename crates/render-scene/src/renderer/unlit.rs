use super::{VertexBufferLayout, VertexStepMode};

/// The vertex buffers of the unlit shader: the vertex and instance streams and a per-instance
/// tint. The retained-UI overlay's pipeline reads the same three.
pub(super) fn alpha_buffers<'a>(
    buffers: &[Option<VertexBufferLayout<'a>>; 2],
    tint_attributes: &'a [wgpu::VertexAttribute],
) -> [Option<VertexBufferLayout<'a>>; 3] {
    [
        buffers[0].clone(),
        buffers[1].clone(),
        Some(VertexBufferLayout {
            array_stride: 16,
            step_mode: VertexStepMode::Instance,
            attributes: tint_attributes,
        }),
    ]
}
