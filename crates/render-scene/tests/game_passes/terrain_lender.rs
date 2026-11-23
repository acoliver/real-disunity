//! A game draws the scene's streamed terrain itself: it declares the terrain surface, so the stock
//! terrain draws stop, and the game's pass draws the chunks the frame's stock selection admitted, from the resident chunk
//! buffers the renderer lends it, with a program of its own.
use super::common::{self, graphics};
use super::fixture::{Rig, SIZE, quad_scene, streamed_terrain_scene, terrain_scene};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, ImportedDraw, ImportedGeometry, ImportedIndices, ImportedVertexInput,
    PreparedProgramDraw, ProgramRuntime,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, DepthAccess, FrameCameraHandle, FrameTerrainError, FrameTerrainHandle,
        GameBuild, GameFeatureFactory, GamePassSpec, GamePasses, GameTargets, PassTarget,
        StockReplacement,
    },
    draw::{TerrainChunkDraw, TerrainChunkEdge},
    geometry::{VERTEX_STRIDE, vertex_attributes},
    wgpu::{self, util::DeviceExt},
};

const TERRAIN: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
const GREEN: [u8; 4] = [0, 255, 0, 255];

/// Positions in scene metres from the lent chunk buffers; the clip transform is one instance.
const TERRAIN_WGSL: &str = "
struct In {
    @location(0) position: vec3<f32>,
    @location(3) c0: vec4<f32>,
    @location(4) c1: vec4<f32>,
    @location(5) c2: vec4<f32>,
    @location(6) c3: vec4<f32>,
}
@vertex fn vs(v: In) -> @builtin(position) vec4<f32> {
    return mat4x4<f32>(v.c0, v.c1, v.c2, v.c3) * vec4<f32>(v.position, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 1.0, 0.0, 1.0);
}";

/// What the frame terrain handle answered in one prepared frame.
type Answer = Result<Option<Vec<TerrainChunkDraw>>, FrameTerrainError>;

/// What the game saw in each frame it prepared.
#[derive(Default)]
struct Seen {
    answers: Vec<Answer>,
}

thread_local! {
    /// The handle the game was built with, kept on the test's thread to read it between frames.
    static HANDLE: std::cell::RefCell<Option<FrameTerrainHandle>> =
        const { std::cell::RefCell::new(None) };
}

type Shared = Arc<Mutex<Seen>>;

struct TerrainGame {
    seen: Shared,
    replaces: StockReplacement,
}

impl GameFeatureFactory for TerrainGame {
    fn name(&self) -> &'static str {
        "terrain game"
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.replaces
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let program = graphics(
            &runtime,
            TERRAIN_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations::default(),
        );
        HANDLE.with_borrow_mut(|handle| *handle = Some(context.terrain.clone()));
        Ok(Box::new(Game {
            targets: context.targets.clone(),
            parts: Rc::new(Parts {
                camera: context.camera.clone(),
                terrain: context.terrain.clone(),
                seen: self.seen.clone(),
                runtime: Mutex::new(runtime),
                program,
                draws: Mutex::new(Vec::new()),
            }),
        }))
    }
}

struct Parts {
    camera: FrameCameraHandle,
    terrain: FrameTerrainHandle,
    seen: Shared,
    runtime: Mutex<ProgramRuntime>,
    program: GraphicsProgram,
    draws: Mutex<Vec<PreparedProgramDraw>>,
}

struct Game {
    targets: GameTargets,
    parts: Rc<Parts>,
}

impl FrameFeature for Game {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
            .writing([ColorOutput::Frame])
            .with_depth(DepthAccess::ReadWrite);
        Ok(FeatureDeclaration {
            passes: vec![self.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(GameFrame {
            parts: self.parts.clone(),
        }))
    }
}

struct GameFrame {
    parts: Rc<Parts>,
}

impl GameFrame {
    fn chunk_draw(
        &self,
        target: &PassTarget,
        (chunk, transform): (&TerrainChunkDraw, &wgpu::Buffer),
    ) -> Result<PreparedProgramDraw, FrameError> {
        let buffers = chunk.buffers();
        let position = vertex_attributes()[0];
        let vertices = ImportedVertexInput {
            buffer: buffers.vertices.clone(),
            stride: VERTEX_STRIDE,
            step: wgpu::VertexStepMode::Vertex,
            attributes: vec![position],
        };
        let instance = ImportedVertexInput {
            buffer: transform.clone(),
            stride: 64,
            step: wgpu::VertexStepMode::Instance,
            attributes: wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4]
                .to_vec(),
        };
        let raster = RasterState {
            cull: None,
            front_face: self.parts.camera.current()?.front_face,
            alpha_to_coverage: false,
        };
        let state = target.render_state(raster);
        let draw = self
            .parts
            .runtime
            .lock()
            .unwrap()
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &self.parts.program,
                    state: &state,
                    vertex_inputs: &[vertices, instance],
                    bindings: &[],
                    imports: &[],
                    sets: &[],
                    geometry: &ImportedGeometry::Indexed(ImportedIndices {
                        buffer: buffers.indices.clone(),
                        format: wgpu::IndexFormat::Uint32,
                        first: 0,
                        count: buffers.index_count,
                        base_vertex: 0,
                    }),
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    instances: 0..1,
                    owners: &[],
                },
                target.signature().unwrap(),
            )
            .map_err(|error| common::program_error("terrain game", &error))?;
        target.admit("terrain game", &draw)?;
        Ok(draw)
    }
}

impl PreparedFeature for GameFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let answer: Answer = self
            .parts
            .terrain
            .current()
            .map(|lent| lent.map(|c| c.to_vec()));
        self.parts.seen.lock().unwrap().answers.push(answer.clone());
        // A game that cannot get the terrain draws none; the frame itself goes on.
        let chunks = answer.ok().flatten().unwrap_or_default();
        let camera = self.parts.camera.current()?;
        let transform = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain game transform"),
                contents: &camera
                    .view_projection
                    .to_cols_array()
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect::<Vec<_>>(),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let target = PassTarget::of(context, TERRAIN)?;
        let draws = chunks
            .iter()
            .map(|chunk| self.chunk_draw(&target, (chunk, &transform)))
            .collect::<Result<Vec<_>, _>>()?;
        *self.parts.draws.lock().unwrap() = draws;
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, TERRAIN);
        for draw in self.parts.draws.lock().unwrap().iter() {
            draw.record(pass);
        }
    }
}

fn centre(image: &[[u8; 4]]) -> [u8; 4] {
    Rig::pixel(image, SIZE[0] as usize / 2, SIZE[1] as usize / 2)
}

/// Renders `frames` frames of `scene` with the terrain game and returns what it saw each frame,
/// the last image and the rig.
fn run(
    scene: &worldrunner_render_plan::RenderScene,
    replaces: StockReplacement,
    frames: usize,
) -> (Vec<Answer>, Vec<[u8; 4]>, Rig) {
    let seen = Shared::default();
    let mut rig = Rig::new(scene, SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(Arc::new(TerrainGame {
            seen: seen.clone(),
            replaces,
        }));
    });
    let mut image = Vec::new();
    for _ in 0..frames {
        image = rig.render().unwrap();
    }
    let answers = std::mem::take(&mut seen.lock().unwrap().answers);
    (answers, image, rig)
}

/// The chunks of the last frame, which has refined past the root.
fn refined(answers: &[Answer]) -> Vec<TerrainChunkDraw> {
    let chunks = answers.last().unwrap().clone().unwrap().unwrap();
    assert!(chunks.len() > 1, "the selection refined past its root");
    chunks
}

#[test]
fn a_game_pass_draws_the_lent_stock_chunks_while_it_declares_the_terrain_surface() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (answers, image, rig) = run(
        &streamed_terrain_scene(),
        StockReplacement::TERRAIN_SURFACE,
        3,
    );
    assert_eq!(centre(&image), GREEN, "the game drew the lent terrain");
    assert_eq!(
        rig.gpu.statistics().passes.terrain.calls,
        0,
        "no stock terrain draw"
    );
    assert_eq!(answers.len(), 3);
    let chunks = refined(&answers);
    assert!(chunks.iter().any(|chunk| {
        [
            TerrainChunkEdge::West,
            TerrainChunkEdge::East,
            TerrainChunkEdge::South,
            TerrainChunkEdge::North,
        ]
        .into_iter()
        .any(|edge| !chunk.neighbors(edge).is_empty())
    }));
    for chunk in &chunks {
        let buffers = chunk.buffers();
        assert!(buffers.index_count > 0);
        assert_eq!(buffers.vertices.size() % VERTEX_STRIDE, 0);
        assert_eq!(buffers.indices.size(), u64::from(buffers.index_count) * 4);
    }
    // Neither between frames nor in a later frame that prepared no terrain does the handle
    // answer with an earlier frame's chunks.
    HANDLE.with_borrow(|handle| {
        let handle = handle.as_ref().unwrap();
        assert_eq!(
            handle.current().unwrap_err(),
            FrameTerrainError::OutsideFrame
        );
        rig.context
            .submission()
            .prepare(rig.context.device(), |_, _| {
                assert_eq!(
                    handle.current().unwrap_err(),
                    FrameTerrainError::NotPrepared
                );
                Ok::<_, FrameError>(())
            })
            .unwrap()
            .cancel();
    });
}

#[test]
fn stock_terrain_draws_and_lends_the_same_frame_selection() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (answers, _, rig) = run(&streamed_terrain_scene(), StockReplacement::NONE, 3);
    assert!(
        rig.gpu.statistics().passes.terrain.calls > 0,
        "the stock pass drew the terrain"
    );
    refined(&answers);
}

#[test]
fn unstreamed_terrain_is_refused_by_name_without_failing_the_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (answers, image, _) = run(&terrain_scene(), StockReplacement::TERRAIN_SURFACE, 1);
    assert_eq!(answers, [Err(FrameTerrainError::NotStreamed)]);
    assert_ne!(centre(&image), GREEN, "the game drew nothing");
}

#[test]
fn a_scene_without_terrain_lends_none() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (answers, _, _) = run(&quad_scene(), StockReplacement::NONE, 1);
    assert_eq!(answers, [Ok(None)]);
}
