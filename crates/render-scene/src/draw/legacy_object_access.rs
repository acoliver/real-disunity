//! What the frozen fixed object path reads from a scene's objects, for the byte-equality
//! captures. Everything here only clones handles the scene already holds.

use super::SceneGpu;
use super::visibility::SelectedRange;
use crate::renderer::legacy_objects::{Draw, Level, Objects, Slot};

impl SceneGpu {
    fn legacy_level(&self, mesh: usize, level: usize) -> Level {
        let lod = &self.meshes[mesh].lods[level];
        Level {
            indices: lod.indices.clone(),
            index_count: lod.index_count,
            slots: self.mesh_selection[mesh].material_slots[level]
                .as_ref()
                .map(|slots| {
                    slots
                        .iter()
                        .map(|slot| {
                            let material = slot.material().raw() as usize;
                            Slot {
                                pipeline: self.instances.pipelines[material],
                                material,
                                indices: slot.indices(),
                            }
                        })
                        .collect()
                }),
        }
    }

    fn legacy_draw(&self, range: &SelectedRange) -> Draw {
        let (pipeline, mesh, material) = range.key;
        let batch = &self.instances.batches[&range.key];
        let lod = &self.meshes[mesh].lods[range.lod];
        Draw {
            pipeline,
            material,
            instances: batch.buffer.clone().expect("flushed instance buffer"),
            previous: batch.previous.clone(),
            vertices: lod.vertices.clone(),
            transition: lod
                .transition
                .clone()
                .map(|vertices| (vertices, self.legacy_level(mesh, range.lod - 1))),
            level: self.legacy_level(mesh, range.lod),
            slots: range.slots.clone(),
        }
    }

    /// The camera view's selected ranges and material groups as the fixed draw reads them now.
    pub(crate) fn legacy_objects(&self) -> Objects {
        let frame = self.drawing_uniform();
        let selection = self.select_objects(None, &frame);
        Objects {
            frame: self.drawing_frame_group(),
            materials: self.materials.clone(),
            draws: selection
                .ranges
                .iter()
                .map(|range| self.legacy_draw(range))
                .collect(),
        }
    }
}
