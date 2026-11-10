//! The fixed object path as the captures drive it: the frozen pipelines of
//! [`crate::renderer::legacy_objects`] built from the frozen source, and the old command
//! sequence over the ranges the scene selects.

use std::ops::Index;

use wgpu::RenderPipeline;

use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::renderer::ScenePipelines;
use crate::renderer::legacy_objects::{Build, Commands, Pipelines, Recorded, record_objects};

/// Pipelines addressed by the index of another key: `index ^ mask`. A mask carries the keys no
/// material reaches (a relief surface, a coverage key without a mask) to the cards that do exist.
pub(in crate::object) struct Masked<'a, P: ?Sized>(pub &'a P, pub usize);

impl<P: Index<usize, Output = RenderPipeline> + ?Sized> Index<usize> for Masked<'_, P> {
    type Output = RenderPipeline;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index ^ self.1]
    }
}

pub(in crate::object) struct Legacy {
    pipelines: Pipelines,
}

impl Legacy {
    /// The fixed pipelines of `selected`, which are the pipelines the scene draws with after its
    /// receiver and raster selections, built from the frozen `source`.
    pub(in crate::object) fn new(
        context: &GpuContext,
        selected: &ScenePipelines,
        source: &str,
    ) -> Self {
        Self {
            pipelines: Pipelines::build(context.device(), &Build::of(selected, source)),
        }
    }

    pub(in crate::object) fn pipelines(&self) -> &Pipelines {
        &self.pipelines
    }

    /// Records the fixed object draw. The caller binds the shadow group at set 2 and the screen
    /// group at set 3 when the pipelines have them.
    pub(in crate::object) fn draw<C: Commands>(
        &self,
        pass: &mut C,
        gpu: &SceneGpu,
        mask: usize,
    ) -> Recorded {
        record_objects(pass, &Masked(&self.pipelines, mask), &gpu.legacy_objects())
    }
}
