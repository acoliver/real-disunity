//! The terrain's draws on the program runtime, prepared once and recorded every frame.
//!
//! A frame records one prepared pipeline and, for each chunk, one geometry. Nothing is built in
//! a steady-state frame: the pipelines, the material sets and the geometry of every resident
//! chunk are made the first time a draw needs them and kept as long as the terrain.
//!
//! Recording stays `2 + 4C` commands for C chunks (`3 + 4C` under a shadow map), the fixed
//! path's count: the pipeline and the material group once, then per chunk two vertex buffers,
//! the index buffer and the draw. The engine's sets are bound by the pass, as before.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use wgpu::{Buffer, Sampler, Texture, TextureView};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{
    GeometryDraw, GraphicsProgram, Import, ImportTable, ImportedGeometry, ImportedIndices,
    Keepalive, PipelineDraw, PreparedPipeline, PreparedSet, ProgramRuntime, VertexBufferSpec,
    VertexLayoutSpec,
};

use super::program::{Context, Surface, semantic};
use super::streaming::TerrainChunk;
use crate::geometry::{INSTANCE_STRIDE, VERTEX_STRIDE, instance_attributes, vertex_attributes};
use crate::terrain::pages::layered_gpu::textures::ResidentSource;

/// What the terrain lends to the runtime, held by every draw made from it.
#[derive(Debug)]
pub(crate) struct Resources {
    pub uniform: Buffer,
    pub planes: Buffer,
    pub sampler: Sampler,
    pub grid: Texture,
    pub grid_view: TextureView,
    /// Albedo, normal and roughness-metallic arrays with their array views.
    pub palettes: Vec<(Texture, TextureView)>,
    pub layered: Option<Arc<ResidentSource>>,
}

/// The page cache and one page table, which the pages surfaces read.
pub(crate) struct Pages {
    pub texture: Texture,
    pub view: TextureView,
    pub table: Buffer,
}

/// What a draw needs to find or make its pipeline.
pub(crate) struct Request<'a> {
    pub context: &'a Context,
    pub authority: &'a SubmissionAuthority,
    pub resources: &'a Arc<Resources>,
    pub surface: Surface,
    /// The frame slot of the page table a pages surface reads; zero otherwise.
    pub slot: usize,
    pub pages: &'a dyn Fn() -> Pages,
}

/// The pipelines, sets and geometry a terrain has prepared.
#[derive(Debug, Default)]
pub(crate) struct PreparedTerrain {
    // A set from another context can belong to a different runtime or material layout.
    sets: RefCell<BTreeMap<(u64, Surface, usize), PreparedSet>>,
    pipelines: RefCell<BTreeMap<(u64, Surface, usize), Rc<PreparedPipeline>>>,
    mesh: RefCell<Option<GeometryDraw>>,
    chunks: RefCell<BTreeMap<TerrainChunk, (Buffer, GeometryDraw)>>,
}

/// How the pipelines read the chunk and instance buffers, which are the fixed path's.
pub(crate) fn vertex_layout() -> VertexLayoutSpec {
    VertexLayoutSpec::new(vec![
        VertexBufferSpec {
            stride: VERTEX_STRIDE,
            step: wgpu::VertexStepMode::Vertex,
            attributes: vertex_attributes().to_vec(),
        },
        VertexBufferSpec {
            stride: INSTANCE_STRIDE,
            step: wgpu::VertexStepMode::Instance,
            attributes: instance_attributes().to_vec(),
        },
    ])
}

fn named(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

/// The resources each binding of a surface's material set is filled from.
fn imports(surface: Surface, resources: &Resources, pages: Option<&Pages>) -> ImportTable {
    let mut table = ImportTable::new()
        .with(Import::buffer(
            named(semantic::BLOCK),
            resources.uniform.clone(),
        ))
        .with(Import::buffer(
            named(semantic::PLANES),
            resources.planes.clone(),
        ));
    if let Some(pages) = pages {
        return table
            .with(Import::texture_array(
                named(semantic::PAGES),
                pages.texture.clone(),
                pages.view.clone(),
            ))
            .with(Import::buffer(
                named(semantic::PAGE_TABLE),
                pages.table.clone(),
            ));
    }
    table.insert(Import::texture(
        named(semantic::TYPE_GRID),
        resources.grid.clone(),
        resources.grid_view.clone(),
    ));
    if let Some(source) = resources.layered.as_ref().filter(|_| surface.layered()) {
        for (name, buffer) in [
            semantic::LAYERED_SOURCE,
            semantic::LAYERED_GRID,
            semantic::LAYERED_IMAGES,
        ]
        .into_iter()
        .zip(&source.buffers)
        {
            table.insert(Import::buffer(named(name), buffer.clone()));
        }
        // The source fills every array slot of the layout, repeating its last array's texture.
        for (index, view) in (0_u32..).zip(&source.views) {
            let texture = &source.textures[usize::try_from(index)
                .expect("slot index")
                .min(source.textures.len() - 1)];
            table.insert(Import::texture_array(
                named(&semantic::layered_array(index)),
                texture.clone(),
                view.clone(),
            ));
        }
        for (name, sampler) in [semantic::LAYERED_REPEAT, semantic::LAYERED_CLAMP]
            .into_iter()
            .zip(&source.samplers)
        {
            table.insert(Import::sampler(named(name), sampler.clone()));
        }
        return table;
    }
    for (name, (texture, view)) in [
        semantic::PALETTE_ALBEDO,
        semantic::PALETTE_NORMAL,
        semantic::PALETTE_ROUGH_METAL,
    ]
    .into_iter()
    .zip(&resources.palettes)
    {
        table.insert(Import::texture_array(
            named(name),
            texture.clone(),
            view.clone(),
        ));
    }
    table.insert(Import::sampler(
        named(semantic::SAMPLER),
        resources.sampler.clone(),
    ));
    table
}

impl PreparedTerrain {
    /// The prepared pipeline for the request, made on first use and found in a map afterwards.
    ///
    /// # Errors
    /// Fails when the runtime refuses the program, the set or the pipeline.
    pub(crate) fn pipeline(
        &self,
        request: &Request<'_>,
    ) -> Result<Rc<PreparedPipeline>, ProgramError> {
        let key = (request.context.id(), request.surface, request.slot);
        if let Some(ready) = self.pipelines.borrow().get(&key) {
            return Ok(ready.clone());
        }
        let context = request.context;
        let made = context
            .family()
            .with((context.device(), request.authority), |runtime| {
                self.prepare(runtime, request)
            })?;
        let made = Rc::new(made);
        self.pipelines.borrow_mut().insert(key, made.clone());
        Ok(made)
    }

    fn prepare(
        &self,
        runtime: &mut ProgramRuntime,
        request: &Request<'_>,
    ) -> Result<PreparedPipeline, ProgramError> {
        let context = request.context;
        let program = context.program(runtime, request.surface)?;
        let mut sets = self.sets.borrow_mut();
        let set_key = (context.id(), request.surface, request.slot);
        if let std::collections::btree_map::Entry::Vacant(vacant) = sets.entry(set_key) {
            vacant.insert(Self::set(runtime, &program, request)?);
        }
        let layout = vertex_layout();
        let state = context.state();
        let owners: [Keepalive; 1] = [request.resources.clone()];
        runtime.prepare_pipeline(
            &PipelineDraw {
                program: &program,
                state: &state,
                layout: &layout,
                bindings: &[],
                imports: &[],
                sets: &[&sets[&set_key]],
                owners: &owners,
            },
            context.signature()?,
        )
    }

    fn set(
        runtime: &mut ProgramRuntime,
        program: &GraphicsProgram,
        request: &Request<'_>,
    ) -> Result<PreparedSet, ProgramError> {
        let pages = request.surface.pages().then(|| (request.pages)());
        let table = imports(request.surface, request.resources, pages.as_ref());
        let owners: [Keepalive; 1] = [request.resources.clone()];
        runtime.prepare_set(program, 1, &table, owners.to_vec())
    }

    /// Draws the one mesh of a terrain that is not streamed.
    pub(crate) fn draw_mesh<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipeline: &PreparedPipeline,
        (vertices, indices, index_count): (&Buffer, &Buffer, u32),
        instances: &Buffer,
    ) {
        let mut mesh = self.mesh.borrow_mut();
        let geometry = mesh.get_or_insert_with(|| {
            GeometryDraw::new(
                &vertex_layout(),
                &[vertices.clone(), instances.clone()],
                &ImportedGeometry::Indexed(ImportedIndices {
                    buffer: indices.clone(),
                    format: wgpu::IndexFormat::Uint32,
                    first: 0,
                    count: index_count,
                    base_vertex: 0,
                }),
                0..1,
            )
            .expect("the terrain mesh fits its buffers")
        });
        pipeline
            .draw(pass, geometry)
            .expect("the mesh was made for the terrain's vertex layout");
    }

    /// Makes the geometry of one chunk of a streamed terrain if it has none for these buffers:
    /// its record `record` of the shared instance buffer is selected by the instance range, so
    /// no slice of the buffer is bound. The streaming state calls this for the chunks it uploads,
    /// in the transaction that uploads them.
    pub(crate) fn prepare_chunk(
        &self,
        chunk: (TerrainChunk, u32),
        (vertices, indices, index_count): (&Buffer, &Buffer, u32),
        instances: &Buffer,
    ) {
        let (chunk, record) = chunk;
        let mut chunks = self.chunks.borrow_mut();
        if chunks
            .get(&chunk)
            .is_some_and(|(known, _)| known == vertices)
        {
            return;
        }
        let geometry = GeometryDraw::new(
            &vertex_layout(),
            &[vertices.clone(), instances.clone()],
            &ImportedGeometry::Indexed(ImportedIndices {
                buffer: indices.clone(),
                format: wgpu::IndexFormat::Uint32,
                first: 0,
                count: index_count,
                base_vertex: 0,
            }),
            record..record + 1,
        )
        .expect("a terrain chunk fits its buffers");
        chunks.insert(chunk, (vertices.clone(), geometry));
    }

    /// Draws one chunk of a streamed terrain through its prepared geometry, which a raw draw
    /// that no preparation preceded makes first.
    pub(crate) fn draw_chunk<P: worldrunner_render_program::runtime::PassCommands>(
        &self,
        pass: &mut P,
        pipeline: &PreparedPipeline,
        chunk: (TerrainChunk, u32),
        lod: (&Buffer, &Buffer, u32),
        instances: &Buffer,
    ) {
        self.prepare_chunk(chunk, lod, instances);
        let chunks = self.chunks.borrow();
        pipeline
            .draw(pass, &chunks[&chunk.0].1)
            .expect("the chunk was made for the terrain's vertex layout");
    }

    /// How many chunks have prepared geometry.
    pub(crate) fn chunk_count(&self) -> usize {
        self.chunks.borrow().len()
    }

    /// Forgets the geometry of chunks that left residency, keeping what a frame in flight may
    /// still draw.
    pub(crate) fn retain_chunks(&self, resident: impl Fn(&TerrainChunk) -> bool) {
        self.chunks.borrow_mut().retain(|chunk, _| resident(chunk));
    }
}
