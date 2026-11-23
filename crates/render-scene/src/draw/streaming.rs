pub(crate) mod casters;
mod chunk_view;
#[cfg(test)]
mod test_hooks;
pub use chunk_view::{
    TerrainChunkBuffers, TerrainChunkDraw, TerrainChunkEdge, TerrainChunkNeighbor,
    TerrainChunkViewError,
};
mod culling;
mod meshes;
mod observations;
mod replacement;
mod residency;
mod residents;
#[cfg(test)]
pub(in crate::draw) mod schedule_tests;
use super::{GpuLod, SceneGpu};
use crate::terrain::prepared::ChunkGeometry;
use crate::terrain::streaming::{
    TerrainChunk, TerrainHeightBounds, TerrainSelection, select_with_retired,
};
#[cfg(test)]
pub(crate) use meshes::await_builds;
use meshes::{Built, MeshBuilds};
use residents::insert;
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_gpu::submission::{Publications, SubmissionAuthority};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_post::{FrameError, PrepareContext};

type Residents = Vec<(TerrainChunk, GpuLod)>;
#[derive(Debug)]
pub(super) struct Streaming {
    scene: RenderScene,
    height_bounds: TerrainHeightBounds,
    plane_chunks: std::collections::BTreeMap<TerrainChunk, u32>,
    identity: Rc<()>,
    pub(super) committed: Rc<RefCell<Residents>>,
    drawing: RefCell<Option<Prepared>>,
    retired: Rc<RefCell<Vec<(u64, Residents)>>>,
    authority: SubmissionAuthority,
    frozen: Rc<std::cell::Cell<bool>>,
    replacement: RefCell<Option<replacement::Replacement>>,
    residency: residency::Residency,
    /// The program geometry of the committed and drawn chunks, and no others.
    geometry: Rc<ChunkGeometry>,
    pub(super) builds: RefCell<MeshBuilds>,
    casters: RefCell<casters::CasterTerrain>,
}
#[derive(Debug)]
struct Prepared {
    owner: Publications,
    matrix: glam::Mat4,
    viewport: [u32; 2],
    residents: Residents,
    observation: crate::terrain::streaming::TerrainSelectionObservation,
    #[cfg(test)]
    uploads: usize,
}
impl Prepared {
    fn matches_view(&self, frame: &crate::frame::FrameUniform, viewport: [u32; 2]) -> bool {
        self.matrix == frame.selection_view_projection() && self.viewport == viewport
    }
}
impl Streaming {
    pub fn new(scene: RenderScene, root: GpuLod, authority: &SubmissionAuthority) -> Self {
        let terrain = scene.terrain().expect("terrain");
        let policy = terrain.streaming_policy().expect("policy");
        let chunk = TerrainChunk::root(terrain, policy);
        let builds = MeshBuilds::new(
            scene.shared_terrain().expect("terrain").clone(),
            policy.uploads_per_frame(),
        );
        Self {
            plane_chunks: crate::terrain::planes::chunk_indices(terrain),
            height_bounds: TerrainHeightBounds::new(terrain),
            scene,
            identity: Rc::new(()),
            committed: Rc::new(RefCell::new(vec![(chunk, root)])),
            drawing: RefCell::new(None),
            retired: Rc::default(),
            authority: authority.clone(),
            frozen: Rc::default(),
            replacement: RefCell::new(None),
            residency: residency::Residency::new(),
            geometry: Rc::default(),
            builds: RefCell::new(builds),
            casters: RefCell::default(),
        }
    }
    fn reuse_prepared(
        &self,
        owner: &Publications,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<bool, FrameError> {
        let drawing = self.drawing.borrow();
        let Some(prepared) = drawing
            .as_ref()
            .filter(|prepared| prepared.owner.same_frame(owner))
        else {
            return Ok(false);
        };
        if !prepared.matches_view(frame, viewport) {
            return Err(FrameError::StreamingSelectionConflict);
        }
        #[cfg(test)]
        record_preparation(prepared, 0);
        Ok(true)
    }
    fn prepare(
        &self,
        context: &mut PrepareContext<'_>,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        self.prepare_completed(context, frame, viewport, self.authority.completed_serial())
    }
    pub(super) fn prepare_completed(
        &self,
        context: &mut PrepareContext<'_>,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
        completed: u64,
    ) -> Result<(), FrameError> {
        if self.replacement.borrow().is_some() {
            return Err(FrameError::FeaturePreparation {
                feature: "streaming terrain",
                reason: "the retained replacement must finish before camera preparation".into(),
            });
        }
        let owner = context.publications();
        if self.reuse_prepared(&owner, frame, viewport)? {
            return Ok(());
        }
        if self.frozen.get() {
            self.prepare_frozen(context, frame, viewport);
            return Ok(());
        }
        *self.drawing.borrow_mut() = None;
        self.retired
            .borrow_mut()
            .retain(|(serial, _)| *serial > completed);
        let old = self.committed.borrow();
        self.release_geometry(&old);
        let current: Vec<_> = old.iter().map(|(key, _)| *key).collect();
        let mut builds = self.builds.borrow_mut();
        let selection = self.select(&mut builds, &current, frame, viewport)?;
        #[cfg(test)]
        let uploads = selection.uploads.len();
        let mut residents: Residents = old
            .iter()
            .filter(|(key, _)| selection.leaves.contains(key))
            .cloned()
            .collect();
        let mut uploaded = false;
        for chunk in &selection.uploads {
            if !selection.leaves.contains(chunk) {
                continue;
            }
            let built = builds.get(*chunk).expect("an admitted chunk has a mesh");
            insert(&mut residents, *chunk, upload_built(context, built)?);
            uploaded = true;
        }
        builds.update(&selection)?;
        self.residency
            .observe(residents.iter().map(|(key, _)| *key), uploaded);
        *self.drawing.borrow_mut() = Some(Prepared {
            owner,
            matrix: frame.selection_view_projection(),
            viewport,
            residents: residents.clone(),
            observation: selection.observation,
            #[cfg(test)]
            uploads,
        });
        #[cfg(test)]
        record_preparation(self.drawing.borrow().as_ref().unwrap(), uploads);
        self.stage_residents(context, residents);
        Ok(())
    }
    /// Ends the geometry's hold on the buffers of every chunk `committed` does not hold. A chunk
    /// that left residency is counted as retired until its frames complete, and a cancelled
    /// frame's uploads are gone with its drawing state, but the geometry's hold on their buffers
    /// is counted nowhere. It ends when the chunk retires and again before uploads are admitted,
    /// which also covers uploads of a frame that never committed.
    fn release_geometry(&self, committed: &Residents) {
        release(&self.geometry, committed);
    }
    /// Chooses the next leaves from the meshes built so far. Tests wait for the builds a
    /// selection asks for, so that a chunk selected in a frame uploads in that frame.
    fn select(
        &self,
        builds: &mut MeshBuilds,
        current: &[TerrainChunk],
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<TerrainSelection, FrameError> {
        builds.collect()?;
        let terrain = self.scene.terrain().expect("terrain");
        let retired = self
            .retired
            .borrow()
            .iter()
            .map(|(_, chunks)| chunks.len())
            .sum();
        let choose = |builds: &MeshBuilds| {
            select_with_retired(
                terrain,
                &self.height_bounds,
                current,
                frame.selection_view_projection(),
                viewport,
                retired,
                &|chunk| builds.is_ready(chunk),
            )
        };
        let selection = choose(builds);
        #[cfg(test)]
        let selection = {
            let mut selection = selection;
            while meshes::awaiting_builds() && !selection.unready.is_empty() {
                builds.update(&selection)?;
                builds.wait_idle();
                builds.collect()?;
                selection = choose(builds);
            }
            selection
        };
        Ok(selection)
    }
    fn stage_residents(&self, context: &PrepareContext<'_>, residents: Residents) {
        let destination = self.committed.clone();
        let retired = self.retired.clone();
        let authority = self.authority.clone();
        let geometry = self.geometry.clone();
        crate::compositor::publication::stage(context, &self.committed, move || {
            let removed = destination
                .borrow()
                .iter()
                .filter(|(key, _)| !residents.iter().any(|(next, _)| next == key))
                .cloned()
                .collect::<Residents>();
            if !removed.is_empty() {
                retired
                    .borrow_mut()
                    .push((authority.submitted_serial(), removed));
            }
            release(&geometry, &residents);
            *destination.borrow_mut() = residents;
        });
    }
    pub(super) fn request(
        &self,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> TerrainGeometryRequest {
        TerrainGeometryRequest {
            identity: self.identity.clone(),
            matrix: frame.selection_view_projection(),
            viewport,
        }
    }
    pub(super) fn ready_for(
        &self,
        request: &TerrainGeometryRequest,
    ) -> Result<bool, crate::terrain::pages::scene::TerrainResidencyError> {
        if !Rc::ptr_eq(&self.identity, &request.identity) {
            return Err(crate::terrain::pages::scene::TerrainResidencyError::Stale);
        }
        if !self.replacement_ready() {
            return Ok(false);
        }
        let terrain = self.scene.terrain().expect("terrain");
        let threshold = f64::from(
            terrain
                .streaming_policy()
                .expect("policy")
                .split_error_millipixels(),
        ) / 1000.0;
        Ok(self.committed.borrow().iter().all(|(chunk, _)| {
            chunk
                .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
                .is_none()
                || crate::terrain::streaming::error(
                    terrain,
                    &self.height_bounds,
                    *chunk,
                    request.matrix,
                    request.viewport,
                ) <= threshold
        }))
    }
    /// The chunks a draw in the transaction `owner` records: the ones prepared in its frame,
    /// including an earlier segment of the same capture, or the committed ones. Every segment of
    /// a frame draws one cover even if an earlier segment's publications replaced the committed
    /// one. `read` also gets the committed ones.
    fn with_residents<R>(
        &self,
        owner: Option<&Publications>,
        read: impl FnOnce(&Residents, &Residents) -> R,
    ) -> R {
        let drawing = self.drawing.borrow();
        let committed = self.committed.borrow();
        let residents = drawing
            .as_ref()
            .filter(|prepared| owner.is_some_and(|owner| owner.same_frame(&prepared.owner)))
            .map_or(&*committed, |prepared| &prepared.residents);
        read(residents, &committed)
    }

    /// Makes the geometry the program runtime draws for the chunks this transaction uploaded, so
    /// that the draw only looks it up. Geometry of chunks that left residency was dropped when
    /// the transaction was prepared, before its uploads were admitted.
    pub(super) fn prepare_geometry(&self, owner: &Publications, terrain: &super::GpuTerrain) {
        self.with_residents(Some(owner), |residents, _| {
            for (chunk, lod) in residents {
                self.geometry.prepare(
                    (*chunk, self.plane_chunks[chunk]),
                    (&lod.vertices, &lod.indices, lod.index_count),
                    &terrain.instances,
                );
            }
        });
    }

    /// Records resident chunks intersecting this view, nearest to farthest. The raster matrix
    /// includes any projection jitter, so a skirt shifted onto the screen remains submitted.
    pub(super) fn draw_prepared<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipeline: &worldrunner_render_program::runtime::PreparedPipeline,
        (terrain, frame): (&super::GpuTerrain, &crate::frame::FrameUniform),
    ) -> super::DrawCount {
        self.record_prepared(
            pass,
            pipeline,
            (terrain, Some(frame.camera_position)),
            self.camera_keep(frame),
        )
    }

    fn camera_keep(
        &self,
        frame: &crate::frame::FrameUniform,
    ) -> impl Fn(&TerrainChunk) -> bool + '_ {
        let scene = self.scene.terrain().expect("terrain");
        culling::keep(scene, &self.height_bounds, frame.view_projection)
    }

    /// Records the resident chunks whose world bounds `keep` accepts, in resident order.
    pub(super) fn draw_prepared_where<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipeline: &worldrunner_render_program::runtime::PreparedPipeline,
        terrain: &super::GpuTerrain,
        keep: impl Fn([glam::Vec3; 2]) -> bool,
    ) -> super::DrawCount {
        let scene = self.scene.terrain().expect("terrain");
        self.record_prepared(pass, pipeline, (terrain, None), |chunk| {
            keep(crate::terrain::streaming::chunk_bounds(scene, *chunk))
        })
    }

    fn record_prepared<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipeline: &worldrunner_render_program::runtime::PreparedPipeline,
        (terrain, eye): (&super::GpuTerrain, Option<glam::Vec3>),
        keep: impl Fn(&TerrainChunk) -> bool,
    ) -> super::DrawCount {
        let mut count = super::DrawCount::default();
        self.with_residents(Publications::current().as_ref(), |residents, _| {
            for (chunk, lod) in self.ordered_where(residents, eye, keep) {
                self.geometry.draw(
                    pass,
                    pipeline,
                    (*chunk, self.plane_chunks[chunk]),
                    (&lod.vertices, &lod.indices, lod.index_count),
                    &terrain.instances,
                );
                count.draw(1);
            }
        });
        count
    }

    fn ordered_where<'a, T>(
        &self,
        residents: &'a [(TerrainChunk, T)],
        eye: Option<glam::Vec3>,
        keep: impl Fn(&TerrainChunk) -> bool,
    ) -> Vec<&'a (TerrainChunk, T)> {
        self.ordered(
            residents.iter().filter(|(chunk, _)| keep(chunk)).collect(),
            eye,
        )
    }

    /// `chunks` from the nearest to `eye` to the farthest, or unchanged without an eye.
    pub(super) fn ordered<'a, T>(
        &self,
        chunks: Vec<&'a (TerrainChunk, T)>,
        eye: Option<glam::Vec3>,
    ) -> Vec<&'a (TerrainChunk, T)> {
        let Some(eye) = eye else {
            return chunks;
        };
        let scene = self.scene.terrain().expect("terrain");
        let keys: Vec<_> = chunks.iter().map(|(chunk, _)| *chunk).collect();
        crate::terrain::streaming::camera_order(scene, &keys, eye)
            .into_iter()
            .map(|index| chunks[index])
            .collect()
    }
}
/// Keeps the geometry of the chunks `committed` holds, with the buffers it holds them in.
fn release(geometry: &ChunkGeometry, committed: &Residents) {
    geometry.retain(|chunk, vertices| {
        committed
            .iter()
            .any(|(key, lod)| key == chunk && lod.vertices == *vertices)
    });
}

#[cfg(test)]
fn record_preparation(prepared: &Prepared, uploads: usize) {
    super::STREAMING_PREPARATIONS.with_borrow_mut(|observations| {
        if let Some(observations) = observations {
            observations.push(super::StreamingPreparation {
                buffers: prepared
                    .residents
                    .iter()
                    .map(|(_, lod)| (lod.vertices.clone(), lod.indices.clone()))
                    .collect(),
                uploads,
            });
        }
    });
}

fn upload_chunk(
    context: &mut PrepareContext<'_>,
    terrain: &worldrunner_render_plan::SceneTerrain,
    chunk: TerrainChunk,
) -> Result<GpuLod, FrameError> {
    upload_built(context, &crate::terrain::chunks::build(terrain, chunk))
}

fn upload_built(context: &mut PrepareContext<'_>, built: &Built) -> Result<GpuLod, FrameError> {
    let mesh = built
        .as_ref()
        .map_err(|error| FrameError::FeaturePreparation {
            feature: "terrain streaming",
            reason: error.to_string(),
        })?;
    Ok(GpuLod {
        vertices: buffer(
            context,
            "terrain chunk vertices",
            wgpu::BufferUsages::VERTEX,
            &mesh.vertices,
        ),
        indices: buffer(
            context,
            "terrain chunk indices",
            wgpu::BufferUsages::INDEX,
            &mesh.indices,
        ),
        index_count: mesh.index_count,
    })
}

fn buffer(
    context: &mut PrepareContext<'_>,
    label: &str,
    usage: wgpu::BufferUsages,
    bytes: &[u8],
) -> wgpu::Buffer {
    let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    context.upload_buffer(&buffer, 0, bytes);
    buffer
}
impl SceneGpu {
    /// Changes whenever the casters a shadow cascade would draw change: placements and
    /// caster membership, or the resident terrain chunk buffers. Both inputs only
    /// increase, so the sum never repeats a value a retained cascade was rendered with.
    pub(crate) fn shadow_caster_generation(&self) -> u64 {
        let terrain = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.streaming.as_ref())
            .map_or(0, |streaming| {
                streaming
                    .residency
                    .generation()
                    .wrapping_add(streaming.casters.borrow().generation())
            });
        self.caster_generation().wrapping_add(terrain)
    }
    pub(crate) fn prepare_frame_geometry(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        self.prepare_terrain_geometry(device, encoder, frame, viewport)?;
        self.prepare_sky(device, encoder, frame)?;
        self.prepare_terrain_pages(device, encoder, frame, viewport)
    }
    /// Prepares the terrain each shadow cascade casts for its fitted light `matrices`, in the
    /// frame `encoder` records, before any cascade pass opens.
    pub(crate) fn prepare_caster_terrain(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        matrices: &[glam::Mat4],
        map_size: u32,
    ) -> Result<(), FrameError> {
        let Some(terrain) = &self.terrain else {
            return Ok(());
        };
        let Some(streaming) = &terrain.streaming else {
            return Ok(());
        };
        let mut context = PrepareContext::new(device, encoder)?;
        streaming.prepare_casters(&mut context, matrices, map_size)?;
        context.finish();
        Ok(())
    }
    /// Whether the scene has terrain, streamed or not.
    pub(crate) fn has_terrain(&self) -> bool {
        self.terrain.is_some()
    }

    /// Prepares the stock streamed terrain selection inside the scene's frame transaction. Stock
    /// scene preparation already performs this step; frames that replace stock scene draws call it
    /// so a retained residency request still advances.
    ///
    /// # Errors
    /// Returns a frame error for a foreign transaction, a conflicting view, or a
    /// terrain upload or preparation failure.
    pub fn prepare_terrain_geometry(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        if let Some(streaming) = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.streaming.as_ref())
        {
            let mut context = PrepareContext::new(device, encoder)?;
            if !streaming
                .authority
                .same_authority(context.publications().authority())
            {
                return Err(FrameError::FeaturePreparation {
                    feature: "streaming terrain",
                    reason: "frame belongs to a different submission authority".into(),
                });
            }
            streaming.prepare(&mut context, frame, viewport)?;
            if let Some(terrain) = &self.terrain {
                streaming.prepare_geometry(&context.publications(), terrain);
            }
            context.finish();
        }
        Ok(())
    }
}

/// Source generation and camera footprint whose selected chunks must be resident.
/// Readiness requires the requested sampling threshold, even under budget pressure.
#[derive(Clone, Debug)]
pub struct TerrainGeometryRequest {
    identity: Rc<()>,
    matrix: glam::Mat4,
    viewport: [u32; 2],
}
impl SceneGpu {
    pub fn terrain_geometry_request(
        &self,
        frame: &crate::frame::FrameUniform,
        viewport: [u32; 2],
    ) -> Option<TerrainGeometryRequest> {
        self.terrain
            .as_ref()
            .and_then(|t| t.streaming.as_ref())
            .map(|s| s.request(frame, viewport))
    }
    pub fn terrain_geometry_ready(
        &self,
        request: &TerrainGeometryRequest,
    ) -> Result<bool, crate::terrain::pages::scene::TerrainResidencyError> {
        self.terrain
            .as_ref()
            .and_then(|t| t.streaming.as_ref())
            .ok_or(crate::terrain::pages::scene::TerrainResidencyError::Stale)?
            .ready_for(request)
    }
}
