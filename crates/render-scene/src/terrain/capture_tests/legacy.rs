//! The fixed terrain path as the captures drive it: the frozen pipelines of
//! [`crate::renderer::legacy_terrain`], bind groups the fixed path built from the same retained
//! resources the scene holds, and the old command sequence.

use wgpu::BindGroup;

use crate::device::GpuContext;
use crate::draw::{DrawCount, PageSlot, SceneGpu};
use crate::renderer::ScenePipelines;
use crate::renderer::legacy_terrain::{
    Build, Commands, Geometry, Pipelines, Resources, Surface, draw_terrain_color,
    terrain_bind_group,
};

pub(in crate::terrain) struct Legacy {
    pipelines: Pipelines,
    inputs: crate::draw::LegacyInputs,
    plain: BindGroup,
    pages: Vec<BindGroup>,
    layered: bool,
}

impl Legacy {
    /// The fixed pipelines of `selected`, which are the pipelines the scene draws with after its
    /// receiver and raster selections, and the bind groups of `gpu`'s terrain.
    pub(in crate::terrain) fn new(
        context: &GpuContext,
        gpu: &SceneGpu,
        selected: &ScenePipelines,
    ) -> Self {
        let device = context.device();
        let pipelines = Pipelines::build(device, &Build::of(selected));
        let inputs = gpu
            .legacy_inputs(device, context.queue())
            .expect("the scene has terrain");
        let layered = gpu.legacy_layered();
        let resources = Resources {
            uniform: &inputs.uniform,
            grid: &inputs.grid,
            palettes: &inputs.palettes,
            layered: inputs.layered.as_deref(),
            planes: &inputs.planes,
            sampler: &inputs.sampler,
        };
        let plain_layout = if layered {
            &pipelines.layered.as_ref().expect("a layered device").layout
        } else {
            &pipelines.layout
        };
        let plain = terrain_bind_group(device, plain_layout, &resources, None);
        let pages = gpu
            .legacy_page_inputs()
            .map_or_else(Vec::new, |(view, tables)| {
                let layout = if layered {
                    &pipelines
                        .layered
                        .as_ref()
                        .expect("a layered device")
                        .cached_layout
                } else {
                    &pipelines.cached_layout
                };
                tables
                    .iter()
                    .map(|table| {
                        terrain_bind_group(device, layout, &resources, Some((&view, table)))
                    })
                    .collect()
            });
        Self {
            pipelines,
            inputs,
            plain,
            pages,
            layered,
        }
    }

    /// Records the fixed terrain draw. The caller binds the frame group at set 0, as the object
    /// pass did, and the screen group at set 3 when the pipelines have one.
    pub(in crate::terrain) fn draw<C: Commands>(
        &self,
        pass: &mut C,
        gpu: &SceneGpu,
        shadow: Option<&BindGroup>,
    ) -> DrawCount {
        let surface = match gpu.legacy_page_slot() {
            PageSlot::Skipped => return DrawCount::default(),
            PageSlot::Slot(slot) => Surface::Pages {
                layered: self.layered,
                group: &self.pages[slot],
            },
            PageSlot::NoPages => Surface::Plain {
                layered: self.layered,
            },
        };
        let chunks;
        let geometry = if self.inputs.streaming {
            chunks = gpu.legacy_chunks();
            Geometry::Streaming(&chunks)
        } else {
            let (vertices, indices, index_count) =
                self.inputs.mesh.as_ref().expect("a nonstreaming mesh");
            Geometry::Mesh {
                vertices,
                indices,
                index_count: *index_count,
            }
        };
        draw_terrain_color(
            pass,
            &self.pipelines,
            &surface,
            &self.plain,
            shadow,
            (&geometry, &self.inputs.instances),
        )
    }
}
