//! The geometry layouts an object program is made for.
//!
//! A skinned level's vertices carry a skin word and its instances an entry in the auxiliary
//! skin stream, so it needs a vertex stage with those inputs, a vertex layout that declares
//! them and a material set that binds the scene palette. The static layout is every other
//! level, and its programs, layouts and sets are the ones that existed before skinning.

use std::sync::OnceLock;

use worldrunner_render_program::runtime::{VertexBufferSpec, VertexLayoutSpec};

use crate::geometry::{
    INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, previous_instance_attributes,
    vertex_attributes,
};
use crate::skinning::{
    SKIN_INSTANCE_BYTES, SKINNED_VERTEX_BYTES, skin_instance_attributes, skinned_vertex_attributes,
};

/// Which vertex format a mesh level is uploaded in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum GeometryLayout {
    #[default]
    Static,
    Skinned,
}

impl GeometryLayout {
    /// The layout of a level that has influences (`skinned`) or has none.
    pub(crate) const fn of(skinned: bool) -> Self {
        if skinned { Self::Skinned } else { Self::Static }
    }

    pub(crate) const fn is_skinned(self) -> bool {
        matches!(self, Self::Skinned)
    }

    /// Bytes of one vertex of this format, which is the stride of the geometry stream.
    pub(crate) const fn vertex_stride(self) -> u64 {
        match self {
            Self::Static => VERTEX_STRIDE,
            Self::Skinned => SKINNED_VERTEX_BYTES as u64,
        }
    }

    /// The geometry stream, slot 0.
    fn vertex_buffer(self) -> VertexBufferSpec {
        let attributes = match self {
            Self::Static => vertex_attributes().to_vec(),
            Self::Skinned => skinned_vertex_attributes().to_vec(),
        };
        VertexBufferSpec {
            stride: self.vertex_stride(),
            step: wgpu::VertexStepMode::Vertex,
            attributes,
        }
    }
}

/// How the pipelines read the mesh, instance and, past those, either the previous-instance
/// buffer of the velocity contexts or the skin stream of a skinned layout.
///
/// A skinned layout has no previous-instance form yet: the velocity contexts refuse it before
/// they ask for a layout, so combining the two here is a programming error.
pub(crate) fn vertex_layout(
    previous_transforms: bool,
    geometry: GeometryLayout,
) -> &'static VertexLayoutSpec {
    static LAYOUTS: OnceLock<[VertexLayoutSpec; 3]> = OnceLock::new();
    assert!(
        !(previous_transforms && geometry.is_skinned()),
        "skinned geometry has no previous-pose vertex layout"
    );
    let layouts = LAYOUTS.get_or_init(|| {
        [
            build_layout(false, GeometryLayout::Static),
            build_layout(true, GeometryLayout::Static),
            build_layout(false, GeometryLayout::Skinned),
        ]
    });
    &layouts[match (previous_transforms, geometry) {
        (false, GeometryLayout::Static) => 0,
        (true, GeometryLayout::Static) => 1,
        (_, GeometryLayout::Skinned) => 2,
    }]
}

fn build_layout(previous_transforms: bool, geometry: GeometryLayout) -> VertexLayoutSpec {
    let mut buffers = vec![
        geometry.vertex_buffer(),
        VertexBufferSpec {
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: instance_attributes().to_vec(),
        },
    ];
    if previous_transforms {
        buffers.push(VertexBufferSpec {
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: previous_instance_attributes().to_vec(),
        });
    } else if geometry.is_skinned() {
        buffers.push(VertexBufferSpec {
            stride: SKIN_INSTANCE_BYTES as u64,
            step: wgpu::VertexStepMode::Instance,
            attributes: skin_instance_attributes().to_vec(),
        });
    }
    VertexLayoutSpec::new(buffers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layouts() -> [(bool, GeometryLayout); 3] {
        [
            (false, GeometryLayout::Static),
            (true, GeometryLayout::Static),
            (false, GeometryLayout::Skinned),
        ]
    }

    #[test]
    fn every_layout_has_unique_locations_that_fit_its_strides() {
        for (previous, geometry) in layouts() {
            let spec = vertex_layout(previous, geometry);
            let mut locations = Vec::new();
            for buffer in spec.buffers() {
                for attribute in &buffer.attributes {
                    let end = attribute.offset + attribute.format.size();
                    assert!(
                        end <= buffer.stride,
                        "{geometry:?} attribute {} ends at {end}, past stride {}",
                        attribute.shader_location,
                        buffer.stride
                    );
                    locations.push(attribute.shader_location);
                }
            }
            let count = locations.len();
            locations.sort_unstable();
            locations.dedup();
            assert_eq!(locations.len(), count, "{geometry:?} repeats a location");
            assert!(locations.iter().all(|&at| at < 16));
        }
    }

    #[test]
    fn the_static_layout_is_the_one_the_fixed_path_declares() {
        let spec = vertex_layout(false, GeometryLayout::Static);
        let strides: Vec<u64> = spec.buffers().iter().map(|buffer| buffer.stride).collect();
        assert_eq!(strides, [56, 80]);
        let velocity = vertex_layout(true, GeometryLayout::Static);
        let strides: Vec<u64> = velocity
            .buffers()
            .iter()
            .map(|buffer| buffer.stride)
            .collect();
        assert_eq!(strides, [56, 80, 80]);
    }

    #[test]
    fn a_skinned_layout_adds_the_skin_word_and_the_skin_stream() {
        let spec = vertex_layout(false, GeometryLayout::Skinned);
        let buffers = spec.buffers();
        assert_eq!(buffers[0].stride, 64);
        assert!(buffers[0].attributes.iter().any(|a| a.shader_location == 9));
        assert_eq!(buffers[2].stride, 16);
        assert_eq!(buffers[2].attributes[0].shader_location, 15);
    }
}
