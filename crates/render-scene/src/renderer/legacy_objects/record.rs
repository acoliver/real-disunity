//! The command sequence the fixed object path records for one view's selected ranges, as
//! `SceneGpu::draw_objects`, `record_lod_range`, `record_lod_draws`, `record_lod_slots` and
//! `BoundState` issued it before objects drew through the program runtime.

use std::ops::{Index, Range};

use wgpu::{BindGroup, Buffer, IndexFormat, RenderPipeline};

use super::Commands;

/// The group index of the frame bindings.
const FRAME_GROUP: u32 = 0;
/// The group index of the material bindings.
const MATERIAL_GROUP: u32 = 1;

/// One draw of a level: the pipeline and material of a material slot and the indices it draws.
pub(crate) struct Slot {
    pub pipeline: usize,
    pub material: usize,
    pub indices: Range<u32>,
}

/// One level of detail of a mesh.
pub(crate) struct Level {
    pub indices: Buffer,
    pub index_count: u32,
    /// The material slots of the level, when it has more than one material.
    pub slots: Option<Vec<Slot>>,
}

/// One selected range: instances of one batch drawn at one level.
pub(crate) struct Draw {
    pub pipeline: usize,
    pub material: usize,
    pub instances: Buffer,
    pub previous: Option<Buffer>,
    pub vertices: Buffer,
    /// The card complement vertices and the level that precedes this one, which draws them.
    pub transition: Option<(Buffer, Level)>,
    pub level: Level,
    pub slots: Range<u32>,
}

/// What the fixed path reads from a scene to record its objects.
pub(crate) struct Objects {
    /// The frame group the draw ran under.
    pub frame: BindGroup,
    /// The material bind group of each material, by index.
    pub materials: Vec<BindGroup>,
    pub draws: Vec<Draw>,
}

/// Instances and draw calls recorded, as `DrawCount` counts them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Recorded {
    pub calls: u64,
    pub instances: u64,
}

/// Render state already set, so repeats are skipped. A pipeline change invalidates the bound
/// material group.
#[derive(Default)]
struct Bound {
    pipeline: Option<usize>,
    material: Option<usize>,
}

impl Bound {
    fn bind(&mut self, pipeline: usize, material: usize) -> (bool, bool) {
        let set_pipeline = self.pipeline != Some(pipeline);
        if set_pipeline {
            self.pipeline = Some(pipeline);
            self.material = None;
        }
        let set_material = self.material != Some(material);
        self.material = Some(material);
        (set_pipeline, set_material)
    }
}

/// Records the frame group and then every selected range, without compaction.
pub(crate) fn record_objects<C: Commands>(
    pass: &mut C,
    pipelines: &(impl Index<usize, Output = RenderPipeline> + ?Sized),
    objects: &Objects,
) -> Recorded {
    pass.set_bind_group(FRAME_GROUP, &objects.frame);
    let mut recorded = Recorded::default();
    let mut bound = Bound::default();
    for draw in &objects.draws {
        pass.set_vertex_buffer(1, &draw.instances);
        // Only velocity pipelines declare this buffer; the others ignore the extra binding.
        if let Some(previous) = &draw.previous {
            pass.set_vertex_buffer(2, previous);
        }
        pass.set_vertex_buffer(0, &draw.vertices);
        record_level(pass, (pipelines, objects), draw, &draw.level, &mut bound)
            .add_to(&mut recorded);
        if let Some((vertices, preceding)) = &draw.transition {
            pass.set_vertex_buffer(0, vertices);
            record_level(pass, (pipelines, objects), draw, preceding, &mut bound)
                .add_to(&mut recorded);
        }
    }
    recorded
}

impl Recorded {
    fn add_to(self, total: &mut Self) {
        total.calls += self.calls;
        total.instances += self.instances;
    }
}

fn record_level<C: Commands>(
    pass: &mut C,
    (pipelines, objects): (
        &(impl Index<usize, Output = RenderPipeline> + ?Sized),
        &Objects,
    ),
    draw: &Draw,
    level: &Level,
    bound: &mut Bound,
) -> Recorded {
    let mut recorded = Recorded::default();
    let instances = u64::from(draw.slots.end - draw.slots.start);
    let mut select = |pass: &mut C, pipeline: usize, material: usize| {
        let (set_pipeline, set_material) = bound.bind(pipeline, material);
        if set_pipeline {
            pass.set_pipeline(&pipelines[pipeline]);
        }
        if set_material {
            pass.set_bind_group(MATERIAL_GROUP, &objects.materials[material]);
        }
    };
    pass.set_index_buffer(&level.indices, IndexFormat::Uint32);
    if let Some(slots) = &level.slots {
        for slot in slots {
            select(pass, slot.pipeline, slot.material);
            pass.draw_indexed(slot.indices.clone(), 0, draw.slots.clone());
            recorded.calls += 1;
            recorded.instances += instances;
        }
    } else {
        select(pass, draw.pipeline, draw.material);
        pass.draw_indexed(0..level.index_count, 0, draw.slots.clone());
        recorded.calls += 1;
        recorded.instances += instances;
    }
    recorded
}
