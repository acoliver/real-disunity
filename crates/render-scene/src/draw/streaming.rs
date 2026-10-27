use super::{DrawError, GpuLod, SceneGpu};
use crate::terrain::{
    TerrainError,
    streaming::{TerrainChunk, select_with_retired},
};
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_gpu::submission::{Publications, SubmissionAuthority};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_post::{FrameError, PrepareContext};

type Residents = Vec<(TerrainChunk, GpuLod)>;
#[derive(Debug)]
pub(super) struct Streaming {
    scene: RenderScene,
    pub(super) committed: Rc<RefCell<Residents>>,
    drawing: RefCell<Option<Prepared>>,
    retired: Rc<RefCell<Vec<(u64, Residents)>>>,
    authority: SubmissionAuthority,
}
#[derive(Debug)]
struct Prepared {
    owner: Publications,
    matrix: glam::Mat4,
    viewport: [u32; 2],
    residents: Residents,
    #[cfg(test)]
    uploads: usize,
}
impl Streaming {
    pub fn new(scene: RenderScene, root: GpuLod, authority: &SubmissionAuthority) -> Self {
        let terrain = scene.terrain().expect("terrain");
        let chunk = TerrainChunk::root(terrain, terrain.streaming_policy().expect("policy"));
        Self {
            scene,
            committed: Rc::new(RefCell::new(vec![(chunk, root)])),
            drawing: RefCell::new(None),
            retired: Rc::default(),
            authority: authority.clone(),
        }
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
        let owner = context.publications();
        if let Some(prepared) = self.drawing.borrow().as_ref()
            && prepared.owner.same_transaction(&owner)
        {
            return if prepared.matrix == frame.view_projection && prepared.viewport == viewport {
                Ok(())
            } else {
                Err(FrameError::StreamingSelectionConflict)
            };
        }
        *self.drawing.borrow_mut() = None;
        self.retired
            .borrow_mut()
            .retain(|(serial, _)| *serial > completed);
        let terrain = self.scene.terrain().expect("terrain");
        let old = self.committed.borrow();
        let current: Vec<_> = old.iter().map(|(key, _)| *key).collect();
        let retired_count = self
            .retired
            .borrow()
            .iter()
            .map(|(_, chunks)| chunks.len())
            .sum();
        let selection = select_with_retired(
            terrain,
            &current,
            frame.view_projection,
            viewport,
            retired_count,
        );
        #[cfg(test)]
        let uploads = selection.uploads.len();
        let mut residents: Residents = old
            .iter()
            .filter(|(key, _)| selection.leaves.contains(key))
            .cloned()
            .collect();
        for chunk in selection.uploads {
            if !selection.leaves.contains(&chunk) {
                continue;
            }
            residents.push((chunk, upload_chunk(context, terrain, chunk)?));
        }
        *self.drawing.borrow_mut() = Some(Prepared {
            owner,
            matrix: frame.view_projection,
            viewport,
            residents: residents.clone(),
            #[cfg(test)]
            uploads,
        });
        self.stage_residents(context, residents);
        Ok(())
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
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, instances: &wgpu::Buffer) {
        let drawing = self.drawing.borrow();
        let committed = self.committed.borrow();
        let residents = drawing
            .as_ref()
            .filter(|prepared| {
                Publications::current()
                    .is_some_and(|current| current.same_transaction(&prepared.owner))
            })
            .map_or(&*committed, |prepared| &prepared.residents);
        pass.set_vertex_buffer(1, instances.slice(..));
        for (_, lod) in residents {
            pass.set_vertex_buffer(0, lod.vertices.slice(..));
            pass.set_index_buffer(lod.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..lod.index_count, 0, 0..1);
        }
    }
}
fn upload_chunk(
    context: &mut PrepareContext<'_>,
    terrain: &worldrunner_render_plan::SceneTerrain,
    chunk: TerrainChunk,
) -> Result<GpuLod, FrameError> {
    let mesh = crate::terrain::chunks::build(terrain, chunk).map_err(|error| {
        FrameError::FeaturePreparation {
            feature: "terrain streaming",
            reason: error.to_string(),
        }
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
pub(super) fn validate(scene: &RenderScene) -> Result<(), DrawError> {
    if scene
        .terrain()
        .is_none_or(|terrain| terrain.streaming_policy().is_none())
    {
        return Ok(());
    }
    let feature = if scene
        .resources()
        .materials()
        .iter()
        .any(|material| material.terrain_contact().is_some())
    {
        Some("contact queries")
    } else if scene.effects().is_some_and(|effects| {
        effects.ssao().is_some() || effects.ssr().is_some_and(|ssr| ssr.enabled)
    }) {
        Some("SSAO and SSR prepasses")
    } else {
        None
    };
    if let Some(feature) = feature {
        return Err(DrawError::Terrain {
            source: TerrainError::StreamingUnsupported { feature },
        });
    }
    Ok(())
}
impl SceneGpu {
    pub(crate) fn reject_streaming(&self, feature: &'static str) -> Result<(), FrameError> {
        if self
            .terrain
            .as_ref()
            .is_some_and(|terrain| terrain.streaming.is_some())
        {
            return Err(FrameError::StreamingTerrainUnsupported(feature));
        }
        Ok(())
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
            context.finish();
        }
        Ok(())
    }
}
