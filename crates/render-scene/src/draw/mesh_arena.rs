//! Pooled vertex and index storage for the scene's mesh levels.
//!
//! A pass that draws many levels would set two buffers per level, and wgpu validates and tracks
//! every one of them. The levels of one upload are packed back to back into chunks instead: one
//! pool of chunks for each vertex format and one for indices. A level is a [`Span`], the chunk
//! and the first vertex or index it holds there, and a draw reads it through `first_index` and
//! `base_vertex` over the chunk's whole buffer. Draws of levels in one chunk set the vertex and
//! index buffers once.
//!
//! * **Pools.** Vertices of different formats cannot share a buffer, because `base_vertex`
//!   counts whole vertices of one stride. A level's card complement repacks the level before it
//!   and is stored in the pool of that level's format. Every index is a `u32`, so one index
//!   pool serves every level and a chunk has one index format.
//! * **Chunks.** An upload packs each level's complement, then its vertices, then its indices,
//!   level by level, so a chunk holds consecutive levels, and splits to a new chunk when the next
//!   level would pass the arena's chunk size, which is [`MAX_CHUNK_BYTES`] or the device's buffer
//!   limit when that is smaller. The last chunk of a pool is exactly as large as what it holds,
//!   so a scene's chunks add up to the bytes its levels packed to, which the budget admission
//!   already counts. A level larger than a chunk gets a chunk of its own.
//! * **Lifetime and holes.** A chunk is reference counted by the spans that read it and is
//!   counted under [`ResourceClass::Mesh`] until the last of them drops. A replaced mesh uploads
//!   into new chunks and drops its old spans. The bytes it leaves in a chunk other meshes still
//!   share stay allocated and are never written again, so a recorded but unsubmitted pass cannot
//!   see them change. The chunk is released when its last level is replaced. The holes of a
//!   scene are bounded by the bytes of its first upload and appends, since a replacement chunk
//!   is released by the next replacement of its mesh.
//! * **Appends.** Each append and replacement packs its own chunks, so a single mesh uploaded
//!   alone fills a chunk of its own, as it filled buffers of its own before.

use std::rc::Rc;

use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device, IndexFormat};
use worldrunner_render_gpu::census::{MemoryCensus, ResourceClass, Tracked};

use super::prepared::PreparedMesh;
use crate::object::layout::GeometryLayout;

/// The format of every index in every chunk.
pub(super) const INDEX_FORMAT: IndexFormat = IndexFormat::Uint32;

/// Bytes of one index.
const INDEX_BYTES: u64 = 4;

/// The most a chunk fills before the next level starts another.
pub(super) const MAX_CHUNK_BYTES: u64 = 64 << 20;

/// One buffer that a pool packed levels into, counted as mesh memory while any span reads it.
#[derive(Debug)]
pub(super) struct Chunk(Tracked<Buffer>);

/// Where a level's vertices or indices are: a chunk, and the first vertex or index it holds.
#[derive(Clone, Debug)]
pub(super) struct Span {
    chunk: Rc<Chunk>,
    first: u32,
}

impl Span {
    /// The buffer of the chunk, which a draw sets whole.
    pub(super) fn buffer(&self) -> &Buffer {
        &self.chunk.0
    }

    /// The vertex or index the level starts at in the chunk.
    pub(super) fn first(&self) -> u32 {
        self.first
    }

    /// The vertex offset a draw adds to the level's indices.
    pub(super) fn base_vertex(&self) -> i32 {
        i32::try_from(self.first).expect("a chunk holds fewer vertices than a draw can offset")
    }
}

/// One mesh level held as spans of the pooled buffers a draw needs.
#[derive(Clone, Debug)]
pub(super) struct MeshLod {
    /// The vertex format of `vertices`.
    pub layout: GeometryLayout,
    /// The level before this one repacked as the complement of an authored card mask, in the
    /// vertex format of that level.
    pub transition: Option<Span>,
    pub vertices: Span,
    pub indices: Span,
    pub index_count: u32,
}

/// One mesh chain, uploaded level by level. Its chunks are counted while it is resident.
#[derive(Debug)]
pub(super) struct GpuMesh {
    pub lods: Vec<MeshLod>,
}

/// A group of chunks whose contents `base_vertex` and `first_index` address alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pool {
    Vertices(GeometryLayout),
    Indices,
}

impl Pool {
    const ALL: [Self; 3] = [
        Self::Vertices(GeometryLayout::Static),
        Self::Vertices(GeometryLayout::Skinned),
        Self::Indices,
    ];

    /// Bytes of the vertex or index a span counts in.
    fn unit(self) -> u64 {
        match self {
            Self::Vertices(layout) => layout.vertex_stride(),
            Self::Indices => INDEX_BYTES,
        }
    }

    fn usage(self) -> BufferUsages {
        match self {
            Self::Vertices(_) => BufferUsages::VERTEX,
            Self::Indices => BufferUsages::INDEX,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Vertices(GeometryLayout::Static) => "mesh arena vertices",
            Self::Vertices(GeometryLayout::Skinned) => "mesh arena skinned vertices",
            Self::Indices => "mesh arena indices",
        }
    }
}

/// Bytes a pool packs for one level, and the pool that holds them.
struct Request<'a> {
    pool: Pool,
    bytes: &'a [u8],
}

/// Where a pool puts each request, by chunk and byte offset, and how large each chunk is.
#[derive(Debug, PartialEq, Eq)]
struct Plan {
    places: Vec<(usize, u64)>,
    chunk_bytes: Vec<u64>,
}

/// Packs `sizes` back to back, starting a chunk when the next one would pass `limit`.
fn plan(sizes: &[u64], limit: u64) -> Plan {
    let mut places = Vec::with_capacity(sizes.len());
    let mut chunk_bytes: Vec<u64> = Vec::new();
    for &size in sizes {
        let chunk = chunk_bytes.len().saturating_sub(1);
        match chunk_bytes.last_mut() {
            Some(used) if size == 0 || *used == 0 || *used + size <= limit => {
                places.push((chunk, *used));
                *used += size;
            }
            _ => {
                places.push((chunk_bytes.len(), 0));
                chunk_bytes.push(size);
            }
        }
    }
    Plan {
        places,
        chunk_bytes,
    }
}

/// The packer of a scene's mesh levels into chunks.
#[derive(Clone, Copy, Debug)]
pub(super) struct MeshArena {
    /// The bytes a chunk may fill before the next level starts another.
    chunk_bytes: u64,
}

impl MeshArena {
    /// An arena whose chunks fill to `chunk_bytes`, which the largest buffer of the device it
    /// uploads to bounds.
    pub(super) fn new(chunk_bytes: u64) -> Self {
        Self { chunk_bytes }
    }

    /// Uploads already packed mesh levels without processing source geometry.
    pub(super) fn upload(
        self,
        device: &Device,
        census: &MemoryCensus,
        meshes: &[PreparedMesh],
    ) -> Vec<GpuMesh> {
        let requests = requests(meshes);
        let mut spans = self.pool_spans(device, census, &requests).into_iter();
        let mut next = || spans.next().expect("a span for every request");
        meshes
            .iter()
            .map(|mesh| GpuMesh {
                lods: mesh
                    .lods
                    .iter()
                    .map(|lod| MeshLod {
                        layout: lod.layout,
                        transition: lod.transition.as_ref().map(|_| next()),
                        vertices: next(),
                        indices: next(),
                        index_count: lod.index_count,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Packs each pool's requests into chunks and returns one span for each request, in order.
    fn pool_spans(
        self,
        device: &Device,
        census: &MemoryCensus,
        requests: &[Request<'_>],
    ) -> Vec<Span> {
        let mut spans: Vec<Option<Span>> = vec![None; requests.len()];
        for pool in Pool::ALL {
            let members: Vec<usize> = (0..requests.len())
                .filter(|&index| requests[index].pool == pool)
                .collect();
            let unit = pool.unit();
            for &member in &members {
                let bytes = requests[member].bytes.len() as u64;
                assert_eq!(bytes % unit, 0, "a level packs whole {pool:?} units");
            }
            let sizes: Vec<u64> = members
                .iter()
                .map(|&member| requests[member].bytes.len() as u64)
                .collect();
            let packed = plan(&sizes, self.chunk_bytes);
            let chunks: Vec<Rc<Chunk>> = packed
                .chunk_bytes
                .iter()
                .enumerate()
                .map(|(chunk, &bytes)| {
                    let items = members
                        .iter()
                        .zip(&packed.places)
                        .filter(|(_, place)| place.0 == chunk)
                        .map(|(&member, place)| (place.1, requests[member].bytes));
                    Rc::new(build_chunk(device, census, pool, bytes, items))
                })
                .collect();
            for (&member, &(chunk, offset)) in members.iter().zip(&packed.places) {
                let first =
                    u32::try_from(offset / unit).expect("a chunk holds fewer than 2^32 units");
                spans[member] = Some(Span {
                    chunk: chunks[chunk].clone(),
                    first,
                });
            }
        }
        spans
            .into_iter()
            .map(|span| span.expect("every request belongs to a pool"))
            .collect()
    }
}

/// The bytes of every level in the order the levels' buffers were made: for each level its
/// complement, then its vertices, then its indices.
fn requests(meshes: &[PreparedMesh]) -> Vec<Request<'_>> {
    let mut requests = Vec::new();
    for mesh in meshes {
        for (level, lod) in mesh.lods.iter().enumerate() {
            if let Some(bytes) = &lod.transition {
                // The complement draws the level before, in that level's vertex format.
                let preceding = mesh.lods[level - 1].layout;
                requests.push(Request {
                    pool: Pool::Vertices(preceding),
                    bytes,
                });
            }
            requests.push(Request {
                pool: Pool::Vertices(lod.layout),
                bytes: &lod.vertices,
            });
            requests.push(Request {
                pool: Pool::Indices,
                bytes: &lod.indices,
            });
        }
    }
    requests
}

/// Makes one chunk of `bytes` bytes and writes each item at its offset.
fn build_chunk<'a>(
    device: &Device,
    census: &MemoryCensus,
    pool: Pool,
    bytes: u64,
    items: impl Iterator<Item = (u64, &'a [u8])>,
) -> Chunk {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(pool.label()),
        size: bytes,
        usage: pool.usage() | BufferUsages::COPY_DST,
        mapped_at_creation: bytes > 0,
    });
    if bytes > 0 {
        {
            let mut view = buffer
                .get_mapped_range_mut(..)
                .expect("a buffer created mapped has a mapped range");
            for (offset, contents) in items {
                let start = usize::try_from(offset).expect("an offset in a mapped chunk");
                view.slice(start..start + contents.len())
                    .copy_from_slice(contents);
            }
        }
        buffer.unmap();
    }
    let entry = census.track(ResourceClass::Mesh, buffer.size());
    Chunk(Tracked::new(buffer, entry))
}

#[cfg(test)]
mod tests;
