//! How a pipeline joins its vertices into primitives.

use crate::error::ProgramError;
use crate::packet::TargetSignature;
use crate::state::{RasterState, RenderState};

/// The topology of a pipeline and, for a strip read through indices, the index format whose
/// largest value restarts the strip.
///
/// wgpu refuses an indexed draw of a strip whose pipeline does not name the index buffer's
/// format, and refuses a pipeline that names a format for a list. So the format is kept for
/// strips only, and two list pipelines that differ only in the index format of their draws are
/// one pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Primitive {
    topology: wgpu::PrimitiveTopology,
    strip_index_format: Option<wgpu::IndexFormat>,
}

impl Primitive {
    /// Separate triangles, the topology of meshes and fullscreen draws.
    pub(crate) const TRIANGLE_LIST: Self = Self {
        topology: wgpu::PrimitiveTopology::TriangleList,
        strip_index_format: None,
    };

    /// `topology` over vertices read through indices of `index`, or in order when it is `None`.
    pub(crate) fn new(topology: wgpu::PrimitiveTopology, index: Option<wgpu::IndexFormat>) -> Self {
        Self {
            topology,
            strip_index_format: index.filter(|_| topology.is_strip()),
        }
    }

    pub(crate) fn state(self, raster: &RasterState) -> wgpu::PrimitiveState {
        wgpu::PrimitiveState {
            topology: self.topology,
            strip_index_format: self.strip_index_format,
            front_face: raster.front_face,
            cull_mode: raster.cull,
            ..wgpu::PrimitiveState::default()
        }
    }

    /// Refuses a depth bias for points and lines, which wgpu applies to triangles only and
    /// refuses when the pipeline is created. Without a depth attachment the bias is not drawn.
    pub(crate) fn check(
        self,
        state: &RenderState,
        signature: TargetSignature,
    ) -> Result<(), ProgramError> {
        let biased = state
            .depth
            .is_some_and(|depth| depth.bias.is_enabled() || depth.bias.clamp != 0.0);
        if biased && signature.depth().is_some() && !self.topology.is_triangles() {
            return Err(ProgramError::DepthBiasWithoutTriangles {
                topology: self.topology,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wgpu::{IndexFormat, PrimitiveTopology};

    use super::Primitive;

    #[test]
    fn a_strip_read_through_indices_keeps_their_format_and_one_read_in_order_has_none() {
        let indexed = Primitive::new(PrimitiveTopology::TriangleStrip, Some(IndexFormat::Uint16));
        assert_eq!(indexed.strip_index_format, Some(IndexFormat::Uint16));
        let lines = Primitive::new(PrimitiveTopology::LineStrip, Some(IndexFormat::Uint32));
        assert_eq!(lines.strip_index_format, Some(IndexFormat::Uint32));
        let in_order = Primitive::new(PrimitiveTopology::TriangleStrip, None);
        assert_eq!(in_order.strip_index_format, None);
    }

    #[test]
    fn a_list_drops_the_index_format_so_lists_over_either_format_are_one_primitive() {
        for topology in [
            PrimitiveTopology::PointList,
            PrimitiveTopology::LineList,
            PrimitiveTopology::TriangleList,
        ] {
            let sixteen = Primitive::new(topology, Some(IndexFormat::Uint16));
            let thirty_two = Primitive::new(topology, Some(IndexFormat::Uint32));
            assert_eq!(sixteen, thirty_two, "{topology:?}");
            assert_eq!(sixteen, Primitive::new(topology, None), "{topology:?}");
        }
        assert_eq!(
            Primitive::new(PrimitiveTopology::TriangleList, Some(IndexFormat::Uint32)),
            Primitive::TRIANGLE_LIST
        );
    }

    #[test]
    fn primitives_that_differ_in_topology_or_strip_index_format_differ() {
        let strip = |index| Primitive::new(PrimitiveTopology::TriangleStrip, index);
        assert_ne!(Primitive::TRIANGLE_LIST, strip(None));
        assert_ne!(
            strip(Some(IndexFormat::Uint16)),
            strip(Some(IndexFormat::Uint32))
        );
        assert_ne!(strip(None), strip(Some(IndexFormat::Uint16)));
    }

    #[test]
    fn the_wgpu_state_carries_the_topology_strip_format_and_raster_winding() {
        let raster = crate::state::RenderState::opaque_depth().raster;
        let state = Primitive::new(PrimitiveTopology::TriangleStrip, Some(IndexFormat::Uint32))
            .state(&raster);
        assert_eq!(state.topology, PrimitiveTopology::TriangleStrip);
        assert_eq!(state.strip_index_format, Some(IndexFormat::Uint32));
        assert_eq!(state.front_face, raster.front_face);
        assert_eq!(state.cull_mode, raster.cull);
    }
}
