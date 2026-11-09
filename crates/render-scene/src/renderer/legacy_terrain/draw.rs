use wgpu::{BindGroup, Buffer, IndexFormat};

use super::Commands;
use super::pipelines::Pipelines;
use crate::draw::DrawCount;
use crate::geometry::INSTANCE_STRIDE;

/// One resident streamed chunk as the fixed draw reads it.
pub(crate) struct Chunk {
    /// The chunk's record in the instance buffer.
    pub record: u32,
    pub vertices: Buffer,
    pub indices: Buffer,
    pub index_count: u32,
}

/// The geometry the fixed draw records.
pub(crate) enum Geometry<'a> {
    Mesh {
        vertices: &'a Buffer,
        indices: &'a Buffer,
        index_count: u32,
    },
    Streaming(&'a [Chunk]),
}

/// Which pipeline and group the fixed draw selected.
pub(crate) enum Surface<'a> {
    Plain { layered: bool },
    Pages { layered: bool, group: &'a BindGroup },
}

/// `draw_terrain_color` and `draw_terrain_geometry` of the fixed path. Returns the draw count
/// and whether terrain was bound.
pub(crate) fn draw_terrain_color<C: Commands>(
    pass: &mut C,
    pipelines: &Pipelines,
    surface: &Surface<'_>,
    plain_group: &BindGroup,
    shadow: Option<&BindGroup>,
    (geometry, instances): (&Geometry<'_>, &Buffer),
) -> DrawCount {
    match surface {
        Surface::Pages { layered, group } => {
            pass.set_pipeline(if *layered {
                &pipelines
                    .layered
                    .as_ref()
                    .expect("validated layered device")
                    .cached
            } else {
                &pipelines.cached
            });
            pass.set_bind_group(1, group);
        }
        Surface::Plain { layered } => {
            pass.set_pipeline(if *layered {
                &pipelines
                    .layered
                    .as_ref()
                    .expect("validated layered device")
                    .terrain
            } else {
                &pipelines.terrain
            });
            pass.set_bind_group(1, plain_group);
        }
    }
    if let Some(shadow) = shadow {
        pass.set_bind_group(2, shadow);
    }
    draw_terrain_geometry(pass, geometry, instances)
}

fn draw_terrain_geometry<C: Commands>(
    pass: &mut C,
    geometry: &Geometry<'_>,
    instances: &Buffer,
) -> DrawCount {
    let mut count = DrawCount::default();
    match geometry {
        Geometry::Streaming(chunks) => {
            for chunk in *chunks {
                let offset = u64::from(chunk.record) * INSTANCE_STRIDE;
                pass.set_vertex_buffer(1, instances, Some(offset..offset + INSTANCE_STRIDE));
                pass.set_vertex_buffer(0, &chunk.vertices, None);
                pass.set_index_buffer(&chunk.indices, IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
                count.draw(1);
            }
        }
        Geometry::Mesh {
            vertices,
            indices,
            index_count,
        } => {
            pass.set_vertex_buffer(0, vertices, None);
            pass.set_vertex_buffer(1, instances, None);
            pass.set_index_buffer(indices, IndexFormat::Uint32);
            pass.draw_indexed(0..*index_count, 0, 0..1);
            count = DrawCount {
                calls: 1,
                instances: 1,
            };
        }
    }
    count
}
