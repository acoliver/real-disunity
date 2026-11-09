//! The dimension and extent the kernels give the render textures they bind as storage images.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::frame::{OffscreenFrame, RenderDimension};

use super::ComputeAdapter;

/// The dimension and layer count or depth of a render texture, as the kernels that bind it as a
/// storage image declare it. The world's description of a render texture has a size and neither
/// a dimension nor a layer count, so the view a kernel declares gives the dimension, and the
/// extent along the layer axis is the most texels that axis any dispatch of a kernel binding it
/// covers: its thread groups along z times the kernel's thread group size along z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageShape {
    pub dimension: RenderDimension,
    pub layers: u32,
}

impl ComputeAdapter {
    /// Records that a kernel binds `id` through a view of `dimension` over `covered` layers or
    /// slices. A texture keeps the dimension it was first bound with; a kernel that declares
    /// another is left out by `StorageDimension` when it is prepared.
    pub(super) fn note_shape(
        &mut self,
        id: TextureId,
        dimension: wgpu::TextureViewDimension,
        covered: u32,
    ) {
        let shape = match dimension {
            wgpu::TextureViewDimension::D2Array => RenderDimension::D2,
            wgpu::TextureViewDimension::D3 => RenderDimension::D3,
            _ => return,
        };
        let held = self.shapes.entry(id).or_insert(StorageShape {
            dimension: shape,
            layers: covered,
        });
        if held.dimension == shape {
            held.layers = held.layers.max(covered);
        }
    }

    /// Gives the render textures of `frame` the shapes the kernels bound them with, now and in
    /// earlier flushes, so that a frame without the dispatch does not make them again as plain
    /// textures and lose what a kernel wrote.
    pub fn shape_textures(&self, frame: &mut OffscreenFrame) {
        for spec in &mut frame.render_textures {
            if let Some(shape) = self.shapes.get(&spec.id) {
                spec.dimension = shape.dimension;
                spec.layers = shape.layers;
            }
        }
    }
}
