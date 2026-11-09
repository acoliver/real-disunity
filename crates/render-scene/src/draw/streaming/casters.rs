//! Terrain the shadow cascades cast from their own light views.
//!
//! A cascade texel covers far more ground than the camera pixel the drawn chunks were refined
//! for, so casting the camera's chunks spends most of the caster passes on triangles the
//! shadow map cannot resolve. Each cascade selects its own chunks for its fitted light matrix
//! and casts from them once every one is drawable: resident for the camera, or built by the
//! mesh worker and uploaded here. Until then the cascade casts the camera's chunks.
use super::meshes::MeshBuilds;
use super::{GpuLod, upload_built};
use crate::terrain::prepared::ChunkGeometry;
use crate::terrain::streaming::{TerrainChunk, caster_leaves};
use glam::Mat4;
use std::collections::{BTreeMap, BTreeSet};
use worldrunner_render_plan::SceneTerrain;
use worldrunner_render_post::{FrameError, PrepareContext};

#[cfg(test)]
thread_local! {
    /// Makes every cascade cast the camera's chunks, as before cascades selected their own.
    pub(crate) static CAMERA_CASTERS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Cascades select their casters for a map this many times smaller than the shadow map. At 2
/// the Stranded slope ridges keep self-shadowing as close to the original's as casting the
/// camera's chunks (ridge error 17.71 against 17.75); at 4 they soften measurably (17.89).
pub(crate) const CASTER_SELECTION_DIVISOR: u32 = 2;

/// One cascade's chunks and what they were selected for.
#[derive(Debug)]
struct Selected {
    matrix: Mat4,
    size: u32,
    camera: Vec<TerrainChunk>,
    leaves: Vec<TerrainChunk>,
}

#[derive(Debug, Default)]
pub(super) struct CasterTerrain {
    /// Each cascade's selection and the light matrix and map size it was made for, so that
    /// still frames do not select again.
    selected: Vec<Option<Selected>>,
    /// Each cascade's own chunks, once all of them are drawable.
    drawn: Vec<Option<Vec<TerrainChunk>>>,
    /// Uploaded meshes of selected chunks the camera does not hold.
    meshes: BTreeMap<TerrainChunk, GpuLod>,
    /// The program geometry of `meshes`, apart from the camera's so that its release rule
    /// leaves these alone.
    pub(super) geometry: ChunkGeometry,
    /// Changes whenever a cascade's casters change, so that retained cascades redraw.
    generation: u64,
}

/// What one frame's caster preparation reads.
pub(super) struct CasterFrame<'a> {
    pub terrain: &'a SceneTerrain,
    pub matrices: &'a [Mat4],
    pub map_size: u32,
    /// Chunks the camera holds for this transaction.
    pub resident: &'a BTreeSet<TerrainChunk>,
    /// The same chunks in a slice, which caster selections are never finer than.
    pub camera: &'a [TerrainChunk],
    pub uploads_per_frame: usize,
    /// The streaming budget. Caster meshes use only what the camera's chunks leave of it.
    pub maximum_resident: usize,
}

impl CasterTerrain {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The chunks cascade `index` casts from, or `None` when it casts the camera's.
    pub fn drawn(&self, index: usize) -> Option<&[TerrainChunk]> {
        self.drawn.get(index)?.as_deref()
    }

    pub fn mesh(&self, chunk: TerrainChunk) -> Option<&GpuLod> {
        self.meshes.get(&chunk)
    }

    /// Selects every cascade's chunks, uploads built meshes within the upload budget,
    /// requests the rest from the worker, and records which cascades can cast their own.
    pub fn prepare(
        &mut self,
        context: &mut PrepareContext<'_>,
        frame: &CasterFrame<'_>,
        builds: &mut MeshBuilds,
    ) -> Result<(), FrameError> {
        let size = (frame.map_size / CASTER_SELECTION_DIVISOR).max(1);
        self.selected.resize_with(frame.matrices.len(), || None);
        for (known, matrix) in self.selected.iter_mut().zip(frame.matrices) {
            if known.as_ref().is_none_or(|known| {
                known.matrix != *matrix || known.size != size || known.camera != frame.camera
            }) {
                *known = Some(Selected {
                    matrix: *matrix,
                    size,
                    camera: frame.camera.to_vec(),
                    leaves: caster_leaves(frame.terrain, *matrix, size, frame.camera),
                });
            }
        }
        let selected: Vec<_> = self
            .selected
            .iter()
            .map(|known| known.as_ref().expect("selected").leaves.clone())
            .collect();
        let needed = needed(&selected, frame.resident, frame.maximum_resident);
        self.meshes.retain(|chunk, _| needed.contains(chunk));
        self.geometry.retain(|chunk, _| needed.contains(chunk));
        builds.collect()?;
        let requests = self.upload(context, &needed, builds, frame.uploads_per_frame)?;
        builds.request_casters(requests)?;
        let drawn: Vec<_> = selected
            .into_iter()
            .map(|leaves| {
                leaves
                    .iter()
                    .all(|chunk| frame.resident.contains(chunk) || self.meshes.contains_key(chunk))
                    .then_some(leaves)
            })
            .collect();
        if drawn != self.drawn {
            self.drawn = drawn;
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }

    /// Uploads built meshes of `needed` chunks, at most `budget`, and returns the chunks not
    /// uploaded yet, so that the worker keeps their built meshes and builds the others.
    fn upload(
        &mut self,
        context: &mut PrepareContext<'_>,
        needed: &BTreeSet<TerrainChunk>,
        builds: &MeshBuilds,
        budget: usize,
    ) -> Result<Vec<TerrainChunk>, FrameError> {
        let mut uploaded = 0;
        let mut requests = Vec::new();
        for chunk in needed {
            if self.meshes.contains_key(chunk) {
                continue;
            }
            match builds.get(*chunk) {
                Some(built) if uploaded < budget => {
                    self.meshes.insert(*chunk, upload_built(context, built)?);
                    uploaded += 1;
                }
                _ => requests.push(*chunk),
            }
        }
        Ok(requests)
    }
}

/// The chunks the cascades need that the camera does not hold, within the room the camera's
/// chunks leave in the streaming budget. Far cascades come first: they need the fewest chunks
/// and their camera chunks cost the most to cast. A cascade whose chunks do not all fit casts
/// the camera's.
fn needed(
    selected: &[Vec<TerrainChunk>],
    resident: &BTreeSet<TerrainChunk>,
    maximum_resident: usize,
) -> BTreeSet<TerrainChunk> {
    let room = maximum_resident.saturating_sub(resident.len());
    let mut needed = BTreeSet::new();
    for leaves in selected.iter().rev() {
        let missing: BTreeSet<_> = leaves
            .iter()
            .filter(|chunk| !resident.contains(chunk) && !needed.contains(*chunk))
            .copied()
            .collect();
        if needed.len() + missing.len() <= room {
            needed.extend(missing);
        }
    }
    needed
}

impl super::Streaming {
    pub(super) fn prepare_casters(
        &self,
        context: &mut PrepareContext<'_>,
        matrices: &[Mat4],
        map_size: u32,
    ) -> Result<(), FrameError> {
        let terrain = self.scene.terrain().expect("terrain");
        let policy = terrain.streaming_policy().expect("policy");
        let resident: BTreeSet<_> = self
            .with_residents(Some(&context.publications()), |residents, _| {
                residents.iter().map(|(chunk, _)| *chunk).collect()
            });
        let camera: Vec<_> = resident.iter().copied().collect();
        let frame = CasterFrame {
            terrain,
            matrices,
            map_size,
            resident: &resident,
            camera: &camera,
            uploads_per_frame: policy.uploads_per_frame() as usize,
            maximum_resident: policy.maximum_resident_chunks() as usize,
        };
        let mut builds = self.builds.borrow_mut();
        let mut casters = self.casters.borrow_mut();
        casters.prepare(context, &frame, &mut builds)?;
        #[cfg(test)]
        if super::meshes::awaiting_builds() {
            // Tests wait for the builds, so that a cascade casts its own chunks in the frame
            // that selects them.
            builds.wait_idle();
            casters.prepare(context, &frame, &mut builds)?;
        }
        Ok(())
    }

    /// Records cascade `index`'s own chunks that `keep` accepts, or returns `None` when the
    /// cascade casts the camera's chunks this frame.
    pub(in crate::draw) fn draw_caster_chunks<
        P: worldrunner_render_program::runtime::PassCommands,
    >(
        &self,
        pass: &mut P,
        pipeline: &worldrunner_render_program::runtime::PreparedPipeline,
        (terrain, index): (&crate::draw::GpuTerrain, u32),
        keep: impl Fn([glam::Vec3; 2]) -> bool,
    ) -> Option<crate::draw::DrawCount> {
        #[cfg(test)]
        if CAMERA_CASTERS.get() {
            return None;
        }
        let scene = self.scene.terrain().expect("terrain");
        let casters = self.casters.borrow();
        let chunks = casters.drawn(index as usize)?;
        self.with_residents(
            worldrunner_render_gpu::submission::Publications::current().as_ref(),
            |residents, _| {
                let resident: BTreeMap<_, _> =
                    residents.iter().map(|(chunk, lod)| (*chunk, lod)).collect();
                let mut draws = Vec::new();
                for chunk in chunks {
                    if !keep(crate::terrain::streaming::chunk_bounds(scene, *chunk)) {
                        continue;
                    }
                    let source = match resident.get(chunk) {
                        Some(lod) => (*lod, &*self.geometry),
                        None => (casters.mesh(*chunk)?, &casters.geometry),
                    };
                    draws.push((*chunk, source));
                }
                let mut count = crate::draw::DrawCount::default();
                for (chunk, (lod, geometry)) in draws {
                    let record = (chunk, self.plane_chunks[&chunk]);
                    let lod = (&lod.vertices, &lod.indices, lod.index_count);
                    geometry.draw(pass, pipeline, record, lod, &terrain.instances);
                    count.draw(1);
                }
                Some(count)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(x: u32, stride: u32) -> TerrainChunk {
        TerrainChunk {
            minimum: [x * 4, 0],
            maximum: [x * 4 + 4, 4],
            stride,
        }
    }

    #[test]
    fn caster_chunks_fit_in_the_room_the_camera_leaves_far_cascades_first() {
        let resident: BTreeSet<_> = [chunk(0, 1), chunk(1, 1)].into();
        let near = vec![chunk(0, 1), chunk(2, 1), chunk(3, 1), chunk(4, 1)];
        let far = vec![chunk(0, 1), chunk(5, 2)];
        let selected = [near.clone(), far];
        // Room for one: only the far cascade's single missing chunk fits.
        assert_eq!(needed(&selected, &resident, 3), [chunk(5, 2)].into());
        // Room for four: both cascades fit, and shared or resident chunks count once.
        assert_eq!(
            needed(&selected, &resident, 6),
            [chunk(2, 1), chunk(3, 1), chunk(4, 1), chunk(5, 2)].into()
        );
        // A full budget leaves every cascade on the camera's chunks.
        assert!(needed(&selected, &resident, 2).is_empty());
        assert!(needed(&[near], &resident, 4).is_empty());
    }
}
