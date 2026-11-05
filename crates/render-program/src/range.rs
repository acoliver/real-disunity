//! The check that a draw's vertex and index ranges lie inside the buffers it binds, made while
//! the draw is prepared so a bad range is refused by name before anything is recorded.
//!
//! The rules are the ones the GPU API applies when the draw is recorded. They are repeated here
//! because a violation there is reported when the frame is finished or panics in the default
//! error handler, long after the offending request.

use crate::draw::{Geometry, VertexInput};
use crate::error::ProgramError;
use crate::resources::Store;

/// How a draw reads one bound vertex buffer, and how many bytes it holds.
#[derive(Clone, Copy)]
pub(crate) struct Fetch<'a> {
    pub(crate) stride: u64,
    pub(crate) step: wgpu::VertexStepMode,
    pub(crate) attributes: &'a [wgpu::VertexAttribute],
    pub(crate) len: u64,
}

/// The vertices a draw reads, with the byte length of the index buffer an indexed draw binds.
#[derive(Clone, Copy)]
pub(crate) enum Extent {
    Vertices {
        first: u32,
        count: u32,
    },
    Indexed {
        buffer_len: u64,
        format: wgpu::IndexFormat,
        first: u32,
        count: u32,
        base_vertex: i32,
    },
}

impl Store {
    /// Checks `geometry` against the vertex buffers of `inputs` and, for an indexed draw, its
    /// index buffer. The draw reads instances up to but not including `instance_end`, so an
    /// instance-rate buffer must hold that many strides; a draw of one instance passes 1.
    ///
    /// The index values themselves are not read: they live in GPU memory. The base vertex is
    /// checked only against the vertices the buffers hold, which refuses a draw whose every
    /// index would land past them.
    ///
    /// # Errors
    /// Fails on an unknown handle, a range whose end overflows, and a range past a buffer.
    pub(crate) fn check_instanced_geometry(
        &self,
        inputs: &[VertexInput],
        geometry: Geometry,
        instance_end: u32,
    ) -> Result<(), ProgramError> {
        let fetches = inputs
            .iter()
            .map(|input| {
                Ok(Fetch {
                    stride: input.stride,
                    step: input.step,
                    attributes: &input.attributes,
                    len: self.buffer(input.buffer)?.len,
                })
            })
            .collect::<Result<Vec<_>, ProgramError>>()?;
        let extent = match geometry {
            Geometry::Vertices { first, count } => Extent::Vertices { first, count },
            Geometry::Indexed {
                buffer,
                format,
                first,
                count,
                base_vertex,
            } => Extent::Indexed {
                buffer_len: self.buffer(buffer)?.len,
                format,
                first,
                count,
                base_vertex,
            },
        };
        check_extent(&fetches, extent, instance_end)
    }
}

/// The rules of [`Store::check_instanced_geometry`] for buffers whose lengths are known.
pub(crate) fn check_extent(
    fetches: &[Fetch<'_>],
    extent: Extent,
    instance_end: u32,
) -> Result<(), ProgramError> {
    let instances = u64::from(instance_end);
    match extent {
        Extent::Vertices { first, count } => {
            let end = range_end(first, count)?;
            check_vertex_buffers(fetches, (u64::from(end), instances))
        }
        Extent::Indexed {
            buffer_len,
            format,
            first,
            count,
            base_vertex,
        } => {
            let end = range_end(first, count)?;
            let available = buffer_len / index_size(format);
            if u64::from(end) > available {
                return Err(ProgramError::IndexRangePastBuffer {
                    end: u64::from(end),
                    available,
                });
            }
            check_vertex_buffers(fetches, (0, instances))?;
            check_base_vertex(fetches, count, base_vertex)
        }
    }
}

/// Every vertex-rate buffer must hold `vertices` vertices and every instance-rate buffer
/// `instances` instances.
fn check_vertex_buffers(
    fetches: &[Fetch<'_>],
    (vertices, instances): (u64, u64),
) -> Result<(), ProgramError> {
    for (slot, fetch) in fetches.iter().enumerate() {
        let strides = match fetch.step {
            wgpu::VertexStepMode::Vertex => vertices,
            wgpu::VertexStepMode::Instance => instances,
        };
        let needed = bytes_read(fetch, strides);
        if needed > fetch.len {
            return Err(ProgramError::VertexRangePastBuffer {
                slot,
                needed,
                have: fetch.len,
            });
        }
    }
    Ok(())
}

fn check_base_vertex(
    fetches: &[Fetch<'_>],
    count: u32,
    base_vertex: i32,
) -> Result<(), ProgramError> {
    let Ok(base) = u64::try_from(base_vertex) else {
        return Ok(());
    };
    for (slot, fetch) in fetches.iter().enumerate() {
        if fetch.step != wgpu::VertexStepMode::Vertex {
            continue;
        }
        let vertices = vertices_held(fetch);
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
fn reach(input: &Fetch<'_>) -> u64 {
    input
        .attributes
        .iter()
        .map(|attribute| attribute.offset.saturating_add(attribute.format.size()))
        .max()
        .unwrap_or(0)
}

/// The bytes of the buffer that reading `strides` strides touches.
fn bytes_read(input: &Fetch<'_>, strides: u64) -> u64 {
    match strides.checked_sub(1) {
        None => 0,
        Some(whole) => whole
            .saturating_mul(input.stride)
            .saturating_add(reach(input)),
    }
}

/// The vertices a buffer serves; a zero stride serves any number.
fn vertices_held(input: &Fetch<'_>) -> u64 {
    if input.stride == 0 {
        return u64::MAX;
    }
    input
        .len
        .checked_sub(reach(input))
        .map_or(0, |spare| spare / input.stride + 1)
}
