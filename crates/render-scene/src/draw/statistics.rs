//! Submitted pass draws, terrain page readiness and authored resource payload sizes.
use super::{SceneGpu, visibility::SelectedRange};

/// Instances count each indexed draw, including material slots and LOD complements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawCount {
    pub calls: u64,
    pub instances: u64,
}

/// Exact selected material pages used to prepare this frame's draw table.
/// Pending includes selected pages not resident for that draw, even if uploaded in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerrainPageStatistics {
    pub requested: usize,
    pub resident: usize,
    pub pending: usize,
}
impl TerrainPageStatistics {
    pub(crate) fn from_residency(pages: impl IntoIterator<Item = bool>) -> Self {
        let mut counts = Self::default();
        for resident in pages {
            counts.requested += 1;
            counts.resident += usize::from(resident);
            counts.pending += usize::from(!resident);
        }
        counts
    }
}

/// Logical payload bytes, excluding driver allocation overhead and staging alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResourceStatistics {
    pub resident_bytes: u64,
    /// Uploads since the previous `take_statistics`, including initial residency.
    pub uploaded_bytes: u64,
}

/// Mesh and impostor fields cover main-camera geometry only. `passes` covers
/// the additional owning passes. Decals shade receivers inline and issue no draws.
/// Texture bytes cover the authored texture table; mesh bytes cover vertex,
/// index and transition buffers, excluding instances and internal resources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneStatistics {
    pub passes: super::PassDrawStatistics,
    pub effects: super::EffectStatistics,
    pub terrain_pages: TerrainPageStatistics,
    /// None before an owning terrain draw, or in a submitted frame without terrain.
    pub terrain_geometry: Option<super::TerrainGeometryStatistics>,
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
    pub reservation: Option<worldrunner_render_gpu::configuration::AllocationReservation>,
    /// Storage admitted for appended and replaced resources, kept until this resident retires.
    pub appended: Vec<worldrunner_render_gpu::configuration::AllocationReservation>,
    pub textures: u64,
    pub meshes: u64,
    /// Payload bytes of each texture and mesh, so a replacement adjusts the totals.
    pub texture_sizes: Vec<u64>,
    pub mesh_sizes: Vec<u64>,
    /// Bytes uploaded by appends and replacements since the previous `take_statistics`.
    pub fresh_textures: u64,
    pub fresh_meshes: u64,
    pub uploaded: bool,
    pub impostors: Vec<bool>,
}
impl ResourceCounters {
    pub fn new(prepared: &super::PreparedScene) -> Self {
        let texture_sizes: Vec<u64> = prepared
            .textures
            .iter()
            .map(|texture| texture.as_prepared().payload_bytes())
            .collect();
        let mesh_sizes: Vec<u64> = prepared
            .meshes
            .iter()
            .map(|mesh| {
                mesh.lods
                    .iter()
                    .map(|lod| {
                        (lod.vertices.len()
                            + lod.indices.len()
                            + lod.transition.as_ref().map_or(0, Vec::len))
                            as u64
                    })
                    .sum()
            })
            .collect();
        Self {
            reservation: None,
            appended: Vec::new(),
            textures: texture_sizes.iter().sum(),
            meshes: mesh_sizes.iter().sum(),
            texture_sizes,
            mesh_sizes,
            fresh_textures: 0,
            fresh_meshes: 0,
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
    /// Keeps scene storage admitted until this resident is retired.
    pub fn retain_resource_reservation(
        &mut self,
        reservation: worldrunner_render_gpu::configuration::AllocationReservation,
    ) {
        self.resource_counters.reservation = Some(reservation);
    }

    /// Reads submitted mesh draws and prepared decal counts without polling or allocation.
    pub fn statistics(&self) -> SceneStatistics {
        let selection = self.views[super::selection_reuse::CAMERA]
            .published
            .borrow();
        let mut stats = SceneStatistics {
            passes: self.pass_counters.snapshot(),
            effects: self.effect_statistics.get(),
            terrain_geometry: self.pass_counters.geometry.get(),
            textures: resource(
                self.resource_counters.textures,
                self.resource_counters.uploaded,
                self.resource_counters.fresh_textures,
            ),
            mesh_buffers: resource(
                self.resource_counters.meshes,
                self.resource_counters.uploaded,
                self.resource_counters.fresh_meshes,
            ),
            terrain_pages: self
                .terrain
                .as_ref()
                .and_then(|terrain| terrain.pages.as_ref())
                .map_or_else(TerrainPageStatistics::default, |pages| {
                    pages.borrow().statistics.get()
                }),
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
        self.resource_counters.fresh_textures = 0;
        self.resource_counters.fresh_meshes = 0;
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
fn resource(bytes: u64, uploaded: bool, fresh: u64) -> ResourceStatistics {
    ResourceStatistics {
        resident_bytes: bytes,
        uploaded_bytes: if uploaded { bytes } else { fresh },
    }
}

impl ResourceCounters {
    pub fn append(&mut self, textures: &[u64], meshes: &[u64]) {
        self.texture_sizes.extend_from_slice(textures);
        self.mesh_sizes.extend_from_slice(meshes);
        self.textures += textures.iter().sum::<u64>();
        self.meshes += meshes.iter().sum::<u64>();
        self.fresh_textures += textures.iter().sum::<u64>();
        self.fresh_meshes += meshes.iter().sum::<u64>();
    }

    pub fn replace_texture(&mut self, index: usize, bytes: u64) {
        let old = std::mem::replace(&mut self.texture_sizes[index], bytes);
        self.textures = self.textures - old + bytes;
        self.fresh_textures += bytes;
    }

    pub fn replace_mesh(&mut self, index: usize, bytes: u64) {
        let old = std::mem::replace(&mut self.mesh_sizes[index], bytes);
        self.meshes = self.meshes - old + bytes;
        self.fresh_meshes += bytes;
    }
}
