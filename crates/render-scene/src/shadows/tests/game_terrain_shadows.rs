//! A game that draws the streamed terrain itself on the shadowed route, which turns the stock
//! terrain draws off for the camera passes, and shades it from the stock shadow cascades. The
//! game's pass samples the lent cascade depth with the lent comparison sampler, so what it
//! shows is the terrain the stock casters wrote into the cascades.
use super::*;
use crate::compositor::{
    ColorOutput, DepthAccess, FrameCameraHandle, FrameInputError, FrameShadowHandle, FrameShadows,
    FrameTerrainError, FrameTerrainHandle, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses,
    GameTargets, PassTarget, StockReplacement,
};
use crate::draw::TerrainChunkDraw;
use crate::geometry::{VERTEX_STRIDE, vertex_attributes};
use crate::offscreen::OffscreenTarget;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedFeature,
};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedIndices, ImportedVertexInput,
    PreparedProgramDraw, ProgramRuntime,
};
use worldrunner_render_program::state::RasterState;

const TERRAIN: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);

/// Streaming refines over several frames, so the scene is compared after the last of them.
const FRAMES: u64 = 4;

/// The game's whole shading model: the point's light-space position from the lent cascade 0
/// matrix, one comparison against the lent cascade depth, and a flat colour for the outcome.
/// Lit terrain is 1.0 in green and shadowed terrain 0.25. A point outside the cascade is lit,
/// as it is for the stock receivers.
const RECEIVER_WGSL: &str = "
struct Shadow {
    view_projection: mat4x4<f32>,
    depth_bias: vec4<f32>,
}
@group(0) @binding(0) var<uniform> shadow: Shadow;
@group(0) @binding(1) var cascades: texture_depth_2d_array;
@group(0) @binding(2) var compare: sampler_comparison;

struct In {
    @location(0) position: vec3<f32>,
    @location(3) c0: vec4<f32>,
    @location(4) c1: vec4<f32>,
    @location(5) c2: vec4<f32>,
    @location(6) c3: vec4<f32>,
}
struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) scene: vec3<f32>,
}
@vertex fn vs(v: In) -> Out {
    var out: Out;
    out.clip = mat4x4<f32>(v.c0, v.c1, v.c2, v.c3) * vec4<f32>(v.position, 1.0);
    out.scene = v.position;
    return out;
}
@fragment fn fs(in: Out) -> @location(0) vec4<f32> {
    let light = shadow.view_projection * vec4<f32>(in.scene, 1.0);
    let ndc = light.xyz / light.w;
    let uv = vec2<f32>(0.5 * ndc.x + 0.5, 0.5 - 0.5 * ndc.y);
    let sampled = textureSampleCompareLevel(
        cascades, compare, uv, 0, ndc.z - shadow.depth_bias.x);
    let inside = all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)) && ndc.z <= 1.0;
    let visible = select(1.0, sampled, inside);
    return vec4<f32>(0.0, select(0.25, 1.0, visible > 0.5), 0.0, 1.0);
}";

/// What the game was lent in one segment that prepared it.
#[derive(Clone, Debug, PartialEq)]
struct Lent {
    chunks: Result<Option<usize>, FrameTerrainError>,
    cascades: Result<usize, FrameInputError>,
}

type Seen = Arc<Mutex<Vec<Lent>>>;

struct Receiver {
    seen: Seen,
    replaces: StockReplacement,
}

struct Parts {
    camera: FrameCameraHandle,
    terrain: FrameTerrainHandle,
    shadows: FrameShadowHandle,
    seen: Seen,
    runtime: Mutex<ProgramRuntime>,
    program: GraphicsProgram,
    draws: Mutex<Vec<PreparedProgramDraw>>,
}

fn binding(at: u32, name: &str, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress {
            set: 0,
            binding: at,
        },
        source: BindingSource::Engine(EngineSemantic::named(name)),
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind,
    }
}

fn receiver_program(runtime: &ProgramRuntime) -> GraphicsProgram {
    let stage = |entry: &str, kind, resources| {
        StageProgram::new(
            StageModule {
                source: StageSource::Wgsl(Arc::from(RECEIVER_WGSL)),
                entry_point: entry.to_owned(),
                kind,
            },
            StageDeclarations {
                resources,
                constants: Vec::new(),
            },
        )
        .unwrap()
    };
    let fragment = ResourceLayout::new(vec![
        binding(0, "terrain.shadow", ResourceKind::Uniform { min_size: 80 }),
        binding(
            1,
            "terrain.cascades",
            ResourceKind::Texture {
                dimension: wgpu::TextureViewDimension::D2Array,
                sample: wgpu::TextureSampleType::Depth,
                multisampled: false,
            },
        ),
        binding(
            2,
            "terrain.compare",
            ResourceKind::Sampler(wgpu::SamplerBindingType::Comparison),
        ),
    ]);
    runtime
        .graphics_program(
            &stage("vs", StageKind::Vertex, ResourceLayout::default()),
            &stage("fs", StageKind::Fragment, fragment),
        )
        .unwrap()
}

impl GameFeatureFactory for Receiver {
    fn name(&self) -> &'static str {
        "terrain receiver"
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
        let program = receiver_program(&runtime);
        Ok(Box::new(Feature {
            targets: context.targets.clone(),
            parts: Rc::new(Parts {
                camera: context.camera.clone(),
                terrain: context.terrain.clone(),
                shadows: context.shadows.clone(),
                seen: self.seen.clone(),
                runtime: Mutex::new(runtime),
                program,
                draws: Mutex::new(Vec::new()),
            }),
        }))
    }
}

struct Feature {
    targets: GameTargets,
    parts: Rc<Parts>,
}

impl FrameFeature for Feature {
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
        Ok(Box::new(Frame {
            parts: self.parts.clone(),
        }))
    }
}

struct Frame {
    parts: Rc<Parts>,
}

fn bytes(floats: &[f32]) -> Vec<u8> {
    floats
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// The three resources the program shades from: cascade 0's matrix and bias, the lent cascade
/// depth and the lent comparison sampler. The fixture has one cascade, so every point the
/// stock receivers shade belongs to it.
fn receiver_imports(device: &wgpu::Device, shadows: &FrameShadows) -> Vec<Import> {
    use wgpu::util::DeviceExt;
    let cascade = shadows.cascades[0];
    let mut words = cascade.view_projection.to_cols_array().to_vec();
    words.extend([cascade.depth_bias, 0.0, 0.0, 0.0]);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("terrain receiver shadow"),
        contents: &bytes(&words),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    vec![
        Import::buffer(EngineSemantic::named("terrain.shadow"), uniform),
        Import::texture_view(
            EngineSemantic::named("terrain.cascades"),
            shadows.depth.texture().clone(),
            shadows.depth.clone(),
            wgpu::TextureViewDimension::D2Array,
        ),
        Import::sampler(
            EngineSemantic::named("terrain.compare"),
            shadows.sampler.clone(),
        ),
    ]
}

impl Frame {
    fn chunk_draw(
        &self,
        target: &PassTarget,
        (chunk, transform): (&TerrainChunkDraw, &wgpu::Buffer),
        imports: &[Import],
    ) -> Result<PreparedProgramDraw, FrameError> {
        let buffers = chunk.buffers();
        let vertices = ImportedVertexInput {
            buffer: buffers.vertices.clone(),
            stride: VERTEX_STRIDE,
            step: wgpu::VertexStepMode::Vertex,
            attributes: vec![vertex_attributes()[0]],
        };
        let instance = ImportedVertexInput {
            buffer: transform.clone(),
            stride: 64,
            step: wgpu::VertexStepMode::Instance,
            attributes: wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4]
                .to_vec(),
        };
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.parts.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
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
                    imports,
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
            .map_err(|error| FrameError::FeaturePreparation {
                feature: "terrain receiver",
                reason: error.to_string(),
            })?;
        target.admit("terrain receiver", &draw)?;
        Ok(draw)
    }
}

impl PreparedFeature for Frame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        use wgpu::util::DeviceExt;
        let parts = &self.parts;
        let chunks = parts.terrain.current();
        let shadows = parts.shadows.current();
        parts.seen.lock().unwrap().push(Lent {
            chunks: chunks
                .as_ref()
                .map(|lent| lent.as_ref().map(|chunks| chunks.len()))
                .map_err(Clone::clone),
            cascades: shadows
                .as_ref()
                .map(|lent| lent.cascades.len())
                .map_err(|e| *e),
        });
        let (Ok(Some(chunks)), Ok(shadows)) = (chunks, shadows) else {
            parts.draws.lock().unwrap().clear();
            return Ok(());
        };
        let imports = receiver_imports(context.device(), &shadows);
        let transform = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain receiver transform"),
                contents: &bytes(&parts.camera.current()?.view_projection.to_cols_array()),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let target = PassTarget::of(context, TERRAIN)?;
        let draws = chunks
            .iter()
            .map(|chunk| self.chunk_draw(&target, (chunk, &transform), &imports))
            .collect::<Result<Vec<_>, _>>()?;
        *parts.draws.lock().unwrap() = draws;
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, TERRAIN);
        for draw in self.parts.draws.lock().unwrap().iter() {
            draw.record(pass);
        }
    }
}

/// Two thin stock quads on the lit side of the ridge, 1.5 m either side of the focus along the
/// view. The depth fit sizes the cascade from the depth the stock camera passes write, and with
/// the terrain left to the game they write only these quads. Each is a centimetre above the
/// ground, so its own shadow is a sliver the size of a pixel at most.
const RECEIVERS: [[i64; 3]; 2] = [[2_000, -1_500, 10], [2_000, 1_500, 10]];

/// A ridge of streamed terrain one metre east of the origin, under a low sun from the east: the
/// ridge's shadow falls on the terrain to its west, and the terrain beyond is lit.
fn ridge_scene() -> RenderScene {
    let source = super::terrain::terrain_scene(&scene(&RECEIVERS, 600_000, None), true, false);
    let terrain = source
        .terrain()
        .unwrap()
        .clone()
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap()));
    source.with_terrain(Some(Arc::new(terrain))).unwrap()
}

/// How many pixels the game drew in each shade.
#[derive(Debug)]
struct Shades {
    lit: usize,
    shadowed: usize,
}

/// The game writes only green: 1.0 for lit terrain and 0.25 for shadowed terrain, which the sRGB
/// target stores as 255 and about 137. The stock sky and the stock lighting write no pixel with
/// so little red and blue.
fn shades(image: &crate::offscreen::RgbaImage) -> Shades {
    let mut shades = Shades {
        lit: 0,
        shadowed: 0,
    };
    for pixel in image.pixels.chunks_exact(4) {
        if pixel[0] > 30 || pixel[2] > 30 {
            continue;
        }
        match pixel[1] {
            200.. => shades.lit += 1,
            100..200 => shades.shadowed += 1,
            _ => {}
        }
    }
    shades
}

/// One frame of the scene as the game sees it, after `FRAMES` frames.
struct Outcome {
    lent: Vec<Lent>,
    shades: Shades,
    stock_terrain_calls: u64,
}

/// Renders `FRAMES` same-frame fitted captures of the ridge with the game installed, declaring
/// `replaces`.
fn run(replaces: StockReplacement) -> Outcome {
    let context = context();
    let mut renderer = shadowed(&context, &ridge_scene(), settings()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: super::sdsm::policy(),
            }),
        )
        .unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    let seen = Seen::default();
    GamePasses::new()
        .with(Arc::new(Receiver {
            seen: seen.clone(),
            replaces,
        }))
        .install(&mut scheduler, &context, None)
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for frame in 0..FRAMES {
        let attachments = (&target.color_attachment(), &target.depth_attachment());
        renderer
            .render_depth_fitted_host(
                &context,
                &view(),
                attachments,
                (Some(frame), false, &[]),
                &mut scheduler,
            )
            .expect("the frame renders without refusal")
            .expect("the capture fitted");
        context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
    }
    let image = target.read_rgba8(&context).unwrap();
    let lent = seen.lock().unwrap().clone();
    Outcome {
        lent,
        shades: shades(&image),
        stock_terrain_calls: renderer.scene_mut().statistics().passes.terrain.calls,
    }
}

/// Every segment that prepared the game was lent chunks and the cascade the fixture declares.
fn assert_lent_everything(outcome: &Outcome, label: &str) {
    assert!(!outcome.lent.is_empty(), "{label}: the game prepared");
    for lent in &outcome.lent {
        assert!(
            matches!(lent.chunks, Ok(Some(count)) if count > 0),
            "{label}: chunks were lent: {lent:?}"
        );
        assert_eq!(lent.cascades, Ok(1), "{label}: shadows were lent");
    }
}

#[test]
fn a_game_drawn_terrain_receives_the_stock_terrain_shadows() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let stock = run(StockReplacement::NONE);
    let declared = run(StockReplacement::TERRAIN_SURFACE);
    assert_lent_everything(&stock, "undeclared");
    assert_lent_everything(&declared, "declared");

    assert!(
        stock.stock_terrain_calls > 0,
        "the stock opaque pass draws terrain, so the count below is not vacuous"
    );
    println!(
        "game pixels, stock {:?}, declared {:?}",
        stock.shades, declared.shades
    );
    assert_eq!(
        declared.stock_terrain_calls, 0,
        "a declared terrain leaves the camera terrain to the game"
    );
    assert!(
        declared.shades.shadowed > 0 && declared.shades.lit > 0,
        "the game's terrain is partly shadowed and partly lit: {:?}",
        declared.shades
    );
}
