//! Submitted main-camera geometry counters and authored resource payload sizes.
use super::{SceneGpu, visibility::SelectedRange};

/// Instances count each indexed draw, including material slots and LOD complements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawCount {
    pub calls: u64,
    pub instances: u64,
}

/// Logical payload bytes, excluding driver allocation overhead and staging alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceStatistics {
    pub resident_bytes: u64,
    /// Uploads since the previous `take_statistics`, including initial residency.
    pub uploaded_bytes: u64,
}

/// Main-camera mesh draws only, excluding shadow/metadata passes, dynamic quads,
/// terrain, water and sky. Decals shade receivers inline and issue no draws.
/// Texture bytes cover the authored texture table; mesh bytes cover vertex,
/// index and transition buffers, excluding instances and internal resources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneStatistics {
    pub meshes: DrawCount,
    pub impostors: DrawCount,
    pub textures: ResourceStatistics,
    pub mesh_buffers: ResourceStatistics,
    /// Selected visible volumes after priority selection and the shader's 32-volume cap.
    pub decals_visible: usize,
    pub decals_total: usize,
}

#[derive(Debug)]
pub(super) struct ResourceCounters {
    pub textures: u64,
    pub meshes: u64,
    pub uploaded: bool,
    pub impostors: Vec<bool>,
}
impl ResourceCounters {
    pub fn new(prepared: &super::PreparedScene) -> Self {
        Self {
            textures: prepared
                .textures
                .iter()
                .map(|texture| {
                    texture
                        .as_prepared()
                        .mips()
                        .iter()
                        .map(|mip| mip.bytes.len() as u64)
                        .sum::<u64>()
                })
                .sum(),
            meshes: prepared
                .meshes
                .iter()
                .flat_map(|mesh| &mesh.lods)
                .map(|lod| {
                    (lod.vertices.len()
                        + lod.indices.len()
                        + lod.transition.as_ref().map_or(0, Vec::len)) as u64
                })
                .sum(),
            uploaded: true,
            impostors: prepared
                .scene
                .resources()
                .materials()
                .iter()
                .map(|material| material.impostor().is_some())
                .collect(),
        }
    }
}
impl SceneGpu {
    /// Reads submitted mesh draws and prepared decal counts without polling or allocation.
    pub fn statistics(&self) -> SceneStatistics {
        let selection = self.camera_selection.borrow();
        let mut stats = SceneStatistics {
            textures: resource(
                self.resource_counters.textures,
                self.resource_counters.uploaded,
            ),
            mesh_buffers: resource(
                self.resource_counters.meshes,
                self.resource_counters.uploaded,
            ),
            decals_visible: self.decal_counts.get()[0],
            decals_total: self.decal_counts.get()[1],
            ..SceneStatistics::default()
        };
        for range in &selection.ranges {
            self.count_lod(range, range.lod, &mut stats);
            if self.meshes[range.key.1].lods[range.lod]
                .transition
                .is_some()
            {
                self.count_lod(range, range.lod - 1, &mut stats);
            }
        }
        stats
    }

    /// Acknowledges authored resource uploads. Repeated reads never consume draw counts.
    pub fn take_statistics(&mut self) -> SceneStatistics {
        let stats = self.statistics();
        self.resource_counters.uploaded = false;
        stats
    }

    fn count_lod(&self, range: &SelectedRange, level: usize, stats: &mut SceneStatistics) {
        let (_, mesh, material) = range.key;
        let instances = u64::from(range.slots.end - range.slots.start);
        if let Some(slots) = &self.mesh_selection[mesh].material_slots[level] {
            for slot in slots {
                self.count_material(slot.material().raw() as usize, instances, stats);
            }
        } else {
            self.count_material(material, instances, stats);
        }
    }

    fn count_material(&self, material: usize, instances: u64, stats: &mut SceneStatistics) {
        let count = if self.resource_counters.impostors[material] {
            &mut stats.impostors
        } else {
            &mut stats.meshes
        };
        count.calls += 1;
        count.instances += instances;
    }
}
fn resource(bytes: u64, uploaded: bool) -> ResourceStatistics {
    ResourceStatistics {
        resident_bytes: bytes,
        uploaded_bytes: if uploaded { bytes } else { 0 },
    }
}
