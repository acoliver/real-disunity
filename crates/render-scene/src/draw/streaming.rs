#[cfg(test)]
pub(crate) mod legacy_access;
mod meshes;
mod observations;
mod replacement;
mod residency;
mod residents;
#[cfg(test)]
pub(in crate::draw) mod schedule_tests;
use super::{GpuLod, SceneGpu};
use crate::terrain::streaming::{TerrainChunk, TerrainSelection, select_with_retired};
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
    plane_chunks: std::collections::BTreeMap<TerrainChunk, u32>,
    identity: Rc<()>,
    pub(super) committed: Rc<RefCell<Residents>>,
    drawing: RefCell<Option<Prepared>>,
    retired: Rc<RefCell<Vec<(u64, Residents)>>>,
    authority: SubmissionAuthority,
    frozen: Rc<std::cell::Cell<bool>>,
    replacement: RefCell<Option<replacement::Replacement>>,
    residency: residency::Residency,
    pub(super) builds: RefCell<MeshBuilds>,
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
            scene,
            identity: Rc::new(()),
            committed: Rc::new(RefCell::new(vec![(chunk, root)])),
            drawing: RefCell::new(None),
            retired: Rc::default(),
            authority: authority.clone(),
            frozen: Rc::default(),
            replacement: RefCell::new(None),
            residency: residency::Residency::new(),
            builds: RefCell::new(builds),
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
        if prepared.matrix != frame.selection_view_projection() || prepared.viewport != viewport {
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
        if self.frozen.get() {
            self.prepare_frozen(context, frame, viewport);
            return Ok(());
        }
        assert!(
            self.replacement.borrow().is_none(),
            "replacement must finish before drawing"
        );
        let owner = context.publications();
        if self.reuse_prepared(&owner, frame, viewport)? {
            return Ok(());
        }
        *self.drawing.borrow_mut() = None;
        self.retired
            .borrow_mut()
            .retain(|(serial, _)| *serial > completed);
        let old = self.committed.borrow();
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
            *destination.borrow_mut() = residents;
        });
    }
    #[cfg(test)]
    pub(super) fn prepared_buffers(&self) -> Vec<wgpu::Buffer> {
        self.drawing
            .borrow()
            .as_ref()
            .unwrap()
            .residents
            .iter()
            .map(|(_, lod)| lod.vertices.clone())
            .collect()
    }
    #[cfg(test)]
    pub(super) fn allocation_counts(&self) -> (usize, usize, usize) {
        let pending = self
            .drawing
            .borrow()
            .as_ref()
            .filter(|prepared| {
                prepared.owner.status()
                    != worldrunner_render_gpu::submission::TransactionStatus::Submitted
            })
            .map_or(0, |prepared| prepared.uploads);
        (
            self.committed.borrow().len(),
            pending,
            self.retired
                .borrow()
                .iter()
                .map(|(_, chunks)| chunks.len())
                .sum(),
        )
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
            chunk.children().is_none()
                || crate::terrain::streaming::error(
                    terrain,
                    *chunk,
                    request.matrix,
                    request.viewport,
                ) <= threshold
        }))
    }
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        instances: &wgpu::Buffer,
    ) -> super::DrawCount {
        self.draw_where(pass, instances, |_| true)
    }

    /// The chunks a draw in the transaction `owner` records: the ones that transaction prepared,
    /// or the committed ones. `read` also gets the committed ones.
    fn with_residents<R>(
        &self,
        owner: Option<&Publications>,
        read: impl FnOnce(&Residents, &Residents) -> R,
    ) -> R {
        let drawing = self.drawing.borrow();
        let committed = self.committed.borrow();
        let residents = drawing
            .as_ref()
            .filter(|prepared| owner.is_some_and(|owner| owner.same_transaction(&prepared.owner)))
            .map_or(&*committed, |prepared| &prepared.residents);
        read(residents, &committed)
    }

    /// Makes the geometry the program runtime draws for the chunks this transaction uploaded, so
    /// that the draw only looks it up. Geometry of chunks that left residency is dropped once the
    /// map holds twice as many chunks as are resident.
    pub(super) fn prepare_geometry(&self, owner: &Publications, terrain: &super::GpuTerrain) {
        self.with_residents(Some(owner), |residents, committed| {
            for (chunk, lod) in residents {
                terrain.prepared.prepare_chunk(
                    (*chunk, self.plane_chunks[chunk]),
                    (&lod.vertices, &lod.indices, lod.index_count),
                    &terrain.instances,
                );
            }
            if terrain.prepared.chunk_count() > 2 * residents.len().max(committed.len()) {
                terrain.prepared.retain_chunks(|chunk| {
                    residents.iter().any(|(known, _)| known == chunk)
                        || committed.iter().any(|(known, _)| known == chunk)
                });
            }
        });
    }

    /// Records the resident chunks through a prepared pipeline, one prepared geometry each. The
    /// chunk's record of the shared instance buffer is its geometry's instance range.
    pub(super) fn draw_prepared<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipeline: &worldrunner_render_program::runtime::PreparedPipeline,
        terrain: &super::GpuTerrain,
    ) -> super::DrawCount {
        let mut count = super::DrawCount::default();
        self.with_residents(Publications::current().as_ref(), |residents, _| {
            for (chunk, lod) in residents {
                terrain.prepared.draw_chunk(
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

    /// Draws the resident chunks whose world bounds `keep` accepts.
    pub fn draw_where(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        instances: &wgpu::Buffer,
        keep: impl Fn(super::visibility::Bounds) -> bool,
    ) -> super::DrawCount {
        let terrain = self.scene.terrain().expect("terrain");
        let mut count = super::DrawCount::default();
        let drawing = self.drawing.borrow();
        let committed = self.committed.borrow();
        let residents = drawing
            .as_ref()
            .filter(|prepared| {
                Publications::current()
                    .is_some_and(|current| current.same_transaction(&prepared.owner))
            })
            .map_or(&*committed, |prepared| &prepared.residents);
        for (chunk, lod) in residents {
            let [min, max] = crate::terrain::streaming::chunk_bounds(terrain, *chunk);
            if !keep(super::visibility::Bounds { min, max }) {
                continue;
            }
            let offset = u64::from(self.plane_chunks[chunk]) * crate::geometry::INSTANCE_STRIDE;
            pass.set_vertex_buffer(
                1,
                instances.slice(offset..offset + crate::geometry::INSTANCE_STRIDE),
            );
            pass.set_vertex_buffer(0, lod.vertices.slice(..));
            pass.set_index_buffer(lod.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..lod.index_count, 0, 0..1);
            count.draw(1);
        }
        count
    }
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
        transition: None,
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
            .map_or(0, |streaming| streaming.residency.generation());
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
    pub(crate) fn prepare_terrain_geometry(
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
