//! The check that a draw's vertex and index ranges lie inside the buffers it binds, made while
//! the draw is prepared so a bad range is refused by name before anything is recorded.
//!
//! The rules are the ones the GPU API applies when the draw is recorded. They are repeated here
//! because a violation there is reported when the frame is finished or panics in the default
//! error handler, long after the offending request.

use crate::draw::{Geometry, VertexInput};
use crate::error::ProgramError;
use crate::resources::Store;

impl Store {
    /// Checks `geometry` against the vertex buffers of `inputs` and, for an indexed draw, its
    /// index buffer. A draw has one instance, so an instance-rate buffer must hold one stride.
    ///
    /// The index values themselves are not read: they live in GPU memory. The base vertex is
    /// checked only against the vertices the buffers hold, which refuses a draw whose every
    /// index would land past them.
    ///
    /// # Errors
    /// Fails on an unknown handle, a range whose end overflows, and a range past a buffer.
    pub(crate) fn check_geometry(
        &self,
        inputs: &[VertexInput],
        geometry: Geometry,
    ) -> Result<(), ProgramError> {
        self.check_instanced_geometry(inputs, geometry, 1)
    }

    /// Like [`check_geometry`](Self::check_geometry) for a draw that reads instances up to but
    /// not including `instance_end`: an instance-rate buffer must hold that many strides.
    pub(crate) fn check_instanced_geometry(
        &self,
        inputs: &[VertexInput],
        geometry: Geometry,
        instance_end: u32,
    ) -> Result<(), ProgramError> {
        let instances = u64::from(instance_end);
        match geometry {
            Geometry::Vertices { first, count } => {
                let end = range_end(first, count)?;
                self.check_vertex_buffers(inputs, (u64::from(end), instances))
            }
            Geometry::Indexed {
                buffer,
                format,
                first,
                count,
                base_vertex,
            } => {
                let end = range_end(first, count)?;
                let available = self.buffer(buffer)?.len / index_size(format);
                if u64::from(end) > available {
                    return Err(ProgramError::IndexRangePastBuffer {
                        end: u64::from(end),
                        available,
                    });
                }
                self.check_vertex_buffers(inputs, (0, instances))?;
                self.check_base_vertex(inputs, count, base_vertex)
            }
        }
    }

    /// Every vertex-rate buffer must hold `vertices` vertices and every instance-rate buffer
    /// `instances` instances.
    fn check_vertex_buffers(
        &self,
        inputs: &[VertexInput],
        (vertices, instances): (u64, u64),
    ) -> Result<(), ProgramError> {
        for (slot, input) in inputs.iter().enumerate() {
            let have = self.buffer(input.buffer)?.len;
            let strides = match input.step {
                wgpu::VertexStepMode::Vertex => vertices,
                wgpu::VertexStepMode::Instance => instances,
            };
            let needed = bytes_read(input, strides);
            if needed > have {
                return Err(ProgramError::VertexRangePastBuffer { slot, needed, have });
            }
        }
        Ok(())
    }

    fn check_base_vertex(
        &self,
        inputs: &[VertexInput],
        count: u32,
        base_vertex: i32,
    ) -> Result<(), ProgramError> {
        let Ok(base) = u64::try_from(base_vertex) else {
            return Ok(());
        };
        for (slot, input) in inputs.iter().enumerate() {
            if input.step != wgpu::VertexStepMode::Vertex {
                continue;
            }
            let vertices = vertices_held(input, self.buffer(input.buffer)?.len);
            if count > 0 && base >= vertices {
                return Err(ProgramError::BaseVertexPastBuffer {
                    slot,
                    base_vertex,
                    vertices,
                });
            }
        }
        Ok(())
    }
}

fn range_end(first: u32, count: u32) -> Result<u32, ProgramError> {
    first
        .checked_add(count)
        .ok_or(ProgramError::DrawRangeOverflow { first, count })
}

fn index_size(format: wgpu::IndexFormat) -> u64 {
    match format {
        wgpu::IndexFormat::Uint16 => 2,
        wgpu::IndexFormat::Uint32 => 4,
    }
}

/// How far into one stride the attributes reach: the last stride of a buffer needs only that
/// much, not the whole stride.
fn reach(input: &VertexInput) -> u64 {
    input
        .attributes
        .iter()
        .map(|attribute| attribute.offset.saturating_add(attribute.format.size()))
        .max()
        .unwrap_or(0)
}

/// The bytes of the buffer that reading `strides` strides touches.
fn bytes_read(input: &VertexInput, strides: u64) -> u64 {
    match strides.checked_sub(1) {
        None => 0,
        Some(whole) => whole
            .saturating_mul(input.stride)
            .saturating_add(reach(input)),
    }
}

/// The vertices a buffer of `len` bytes serves; a zero stride serves any number.
fn vertices_held(input: &VertexInput, len: u64) -> u64 {
    if input.stride == 0 {
        return u64::MAX;
    }
    len.checked_sub(reach(input))
        .map_or(0, |spare| spare / input.stride + 1)
}
