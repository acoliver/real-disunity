//! Vertex streams of a fullscreen operation.
use super::{
    AccessMode, BufferUse, BufferUseKind, BufferView, ContractError, ResourceRegistry,
    geometry::check_slice,
};

/// A vertex stream of a fullscreen operation and the bytes of it one vertex reads.
#[derive(Clone, Debug)]
pub struct VertexStream {
    pub view: BufferView,
    pub stride: u64,
    /// The end of the furthest attribute within one vertex.
    pub extent: u64,
}
/// The streams and vertex count of a fullscreen operation that reads vertex buffers. The
/// operation draws `count` vertices of one instance, with stream `n` in vertex slot `n`.
#[derive(Clone, Debug)]
pub struct FullscreenVertices {
    pub streams: Vec<VertexStream>,
    pub count: u32,
}
impl FullscreenVertices {
    /// Every stream is a vertex buffer slice that holds `count` vertices and that the
    /// operation declares it reads.
    pub(super) fn validate(
        &self,
        registry: &ResourceRegistry,
        declared: &[BufferUse],
    ) -> Result<(), ContractError> {
        if self.count == 0 {
            return Err(ContractError::Pipeline);
        }
        for stream in &self.streams {
            let view = &stream.view;
            registry.check_buffer(view)?;
            check_slice(view, 4, wgpu::BufferUsages::VERTEX)?;
            let required = u64::from(self.count - 1)
                .checked_mul(stream.stride)
                .and_then(|offset| offset.checked_add(stream.extent))
                .ok_or(ContractError::Range(view.allocation()))?;
            if required > view.range.end - view.range.start {
                return Err(ContractError::Range(view.allocation()));
            }
            if !declared.iter().any(|usage| {
                usage.view.allocation == view.allocation
                    && usage.view.version == view.version
                    && usage.view.range.start <= view.range.start
                    && usage.view.range.end >= view.range.end
                    && usage.kind == BufferUseKind::Vertex
                    && usage.access == AccessMode::Read
            }) {
                return Err(ContractError::Binding(view.allocation()));
            }
        }
        Ok(())
    }
    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        for (slot, stream) in self.streams.iter().enumerate() {
            pass.set_vertex_buffer(
                u32::try_from(slot).expect("checked vertex slot"),
                stream.view.buffer.slice(stream.view.range.clone()),
            );
        }
        pass.draw(0..self.count, 0..1);
    }
}
