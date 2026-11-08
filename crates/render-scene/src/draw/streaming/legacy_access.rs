//! What the frozen fixed terrain path reads from a scene's terrain, for the byte-equality
//! captures. Everything here only clones handles the scene already holds.

use super::{Publications, Streaming};
use crate::draw::SceneGpu;
use crate::renderer::legacy_terrain::Chunk;
use crate::terrain::pages::layered_gpu::textures::ResidentSource;
use std::sync::Arc;

/// Which surface the fixed draw selects.
pub(crate) enum PageSlot {
    /// No material pages are enabled: the terrain draws from its own bind group.
    NoPages,
    /// Pages are enabled but the root page is not resident: terrain is skipped for the frame.
    Skipped,
    /// The frame slot whose page table is drawn.
    Slot(usize),
}

/// The resources the fixed bind groups and draws are made from.
pub(crate) struct LegacyInputs {
    pub uniform: wgpu::Buffer,
    pub grid: wgpu::Texture,
    pub palettes: Vec<wgpu::Texture>,
    pub layered: Option<Arc<ResidentSource>>,
    pub planes: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    pub instances: wgpu::Buffer,
    /// The vertex buffer, index buffer and index count of a non-streaming mesh.
    pub mesh: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    pub streaming: bool,
}

impl Streaming {
    /// The resident chunks the fixed draw would record now: the drawing transaction's when the
    /// current transaction prepared them, else the committed ones.
    pub(crate) fn legacy_chunks(&self) -> Vec<Chunk> {
        let drawing = self.drawing.borrow();
        let committed = self.committed.borrow();
        let residents = drawing
            .as_ref()
            .filter(|prepared| {
                Publications::current()
                    .is_some_and(|current| current.same_transaction(&prepared.owner))
            })
            .map_or(&*committed, |prepared| &prepared.residents);
        residents
            .iter()
            .map(|(chunk, lod)| Chunk {
                record: self.plane_chunks[chunk],
                vertices: lod.vertices.clone(),
                indices: lod.indices.clone(),
                index_count: lod.index_count,
            })
            .collect()
    }
}

impl SceneGpu {
    /// The scene terrain's resources as the fixed path was handed them, or `None` without
    /// terrain.
    pub(crate) fn legacy_inputs(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<LegacyInputs> {
        let terrain = self.terrain.as_ref()?;
        Some(LegacyInputs {
            uniform: terrain.uniform.clone(),
            grid: terrain.retained[3].clone(),
            palettes: terrain.retained[..3].to_vec(),
            layered: terrain
                .layered
                .as_ref()
                .map(|source| source.resident(device, queue).expect("source admitted")),
            planes: terrain.triangle_source.clone(),
            sampler: terrain.sampler.clone(),
            instances: terrain.instances.clone(),
            mesh: terrain
                .lod
                .as_ref()
                .map(|lod| (lod.vertices.clone(), lod.indices.clone(), lod.index_count)),
            streaming: terrain.streaming.is_some(),
        })
    }

    /// The chunks of a streaming terrain, as the fixed draw reads them now.
    pub(crate) fn legacy_chunks(&self) -> Vec<Chunk> {
        self.terrain
            .as_ref()
            .and_then(|terrain| terrain.streaming.as_ref())
            .map(Streaming::legacy_chunks)
            .expect("streaming terrain")
    }

    /// The page cache view and the page tables of the terrain's material pages, if enabled.
    pub(crate) fn legacy_page_inputs(&self) -> Option<(wgpu::TextureView, Vec<wgpu::Buffer>)> {
        let pages = self.terrain.as_ref()?.pages.as_ref()?.borrow();
        Some((pages.cache.texture_view(), pages.tables.clone()))
    }

    /// What the fixed draw sees of material pages. Call it in the transaction that prepared
    /// the pages.
    pub(crate) fn legacy_page_slot(&self) -> PageSlot {
        let Some(pages) = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.pages.as_ref())
        else {
            return PageSlot::NoPages;
        };
        pages
            .borrow()
            .draw_slot()
            .map_or(PageSlot::Skipped, PageSlot::Slot)
    }

    /// Whether the terrain is layered.
    pub(crate) fn legacy_layered(&self) -> bool {
        self.terrain
            .as_ref()
            .is_some_and(|terrain| terrain.layered.is_some())
    }

    /// The frame group the fixed draw ran under.
    pub(crate) fn legacy_frame_group(&self) -> wgpu::BindGroup {
        self.drawing_frame_group()
    }
}
