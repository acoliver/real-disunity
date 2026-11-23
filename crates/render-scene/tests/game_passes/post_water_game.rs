//! A test game that reads the post-water depth from a pass at `GamePlace::AfterTransparent`, and
//! can write depth into the frame ahead of it from a pass at `GamePlace::AfterOpaqueSnapshots`.
use super::common::{self, texture_binding};
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
};
use worldrunner_render_program::binding::{EngineSemantic, ResourceLayout};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, PreparedProgramDraw, ProgramRuntime,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, DepthAccess, FrameSnapshotError, FrameSnapshotHandle, FrameSnapshots,
        GameBuild, GameFeatureFactory, GamePassSpec, GamePasses, GameTargets, PassTarget,
        PostWaterDepth, SnapshotImage, SnapshotReads, StockReplacement, TargetDeclaration,
        TargetExtent,
    },
    wgpu,
};

/// What the game saw, in the order it saw it.
#[derive(Default)]
pub struct Seen {
    /// The entry of every pass the game recorded into.
    pub recorded: Vec<PassEntry>,
    /// What the handle lent in each frame's `prepare`.
    pub lent: Vec<Result<Lent, FrameSnapshotError>>,
}

pub type Shared = Arc<Mutex<Seen>>;

/// Which depth snapshots a frame lent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lent {
    pub linear_depth: bool,
    pub device_depth: bool,
    pub post_water_depth: bool,
    pub post_water_device_depth: bool,
}

/// The depth the writer gives the left half of the frame. It is exactly representable, and in the
/// fixture's orthographic view it is `0.1 + 0.25 * 9.9 = 2.575` metres.
pub const WRITTEN_DEPTH: f32 = 0.25;

/// A game target the reader writes: the post-water device depth in red, the post-water linear
/// depth in green, the opaque device depth in blue and the opaque linear depth in alpha. A reader
/// of the linear post-water depth alone writes it in green and leaves the rest zero.
pub const PROBE: &str = "post-water game probe";

pub const WRITER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 0);
/// A pass that reads the post-water depth too early: nothing has produced it yet.
pub const EARLY_READER: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 1);
pub const READER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterTransparent), 0);

const FULLSCREEN: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.25, 1.0);
}";

/// Draws the left half of the 64 pixel wide frame at depth [`WRITTEN_DEPTH`].
const WRITE_LEFT: &str = "
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    if p.x >= 32.0 { discard; }
    return vec4<f32>(1.0, 0.0, 1.0, 1.0);
}";

const MAGENTA: &str = "
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0, 0.0, 1.0, 1.0); }";

const READ_LINEAR: &str = "
@group(0) @binding(0) var post_linear: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, textureLoad(post_linear, vec2<i32>(p.xy), 0).r, 0.0, 0.0);
}";

const READ_OPAQUE: &str = "
@group(0) @binding(0) var opaque_device: texture_2d<f32>;
@group(0) @binding(1) var opaque_linear: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    return vec4<f32>(0.0, 0.0, textureLoad(opaque_device, pixel, 0).r, textureLoad(opaque_linear, pixel, 0).r);
}";

const READ_ALL: &str = "
@group(0) @binding(0) var post_device: texture_2d<f32>;
@group(0) @binding(1) var post_linear: texture_2d<f32>;
@group(0) @binding(2) var opaque_device: texture_2d<f32>;
@group(0) @binding(3) var opaque_linear: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    return vec4<f32>(
        textureLoad(post_device, pixel, 0).r,
        textureLoad(post_linear, pixel, 0).r,
        textureLoad(opaque_device, pixel, 0).r,
        textureLoad(opaque_linear, pixel, 0).r,
    );
}";

/// What the reader samples.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The linear post-water depth alone.
    Linear,
    /// The opaque depths alone.
    Opaque,
    /// Both post-water depths and both opaque depths.
    All,
}

impl Reading {
    pub fn reads(self) -> SnapshotReads {
        match self {
            Self::Linear => SnapshotReads {
                post_water_depth: PostWaterDepth::Linear,
                ..SnapshotReads::NONE
            },
            Self::Opaque => SnapshotReads {
                linear_depth: true,
                device_depth: true,
                ..SnapshotReads::NONE
            },
            Self::All => SnapshotReads {
                linear_depth: true,
                device_depth: true,
                post_water_depth: PostWaterDepth::LinearAndDevice,
                ..SnapshotReads::NONE
            },
        }
    }
}

/// What the game plans each frame.
#[derive(Clone, Copy, Default)]
pub struct Plan {
    /// Writes [`WRITTEN_DEPTH`] into the left half of the frame's depth at [`WRITER`].
    pub write_depth: bool,
    /// Reads the post-water depth at [`EARLY_READER`], before anything has produced it.
    pub early_reader: bool,
    /// Samples the depths at [`READER`] into [`PROBE`].
    pub reader: Option<Reading>,
    /// The stock surfaces the game draws.
    pub replaces: StockReplacement,
}

#[derive(Clone)]
pub struct PostWaterGame {
    /// What the factory declares it reads. A pass may read only this.
    pub declared: SnapshotReads,
    pub plan: Plan,
    pub seen: Arc<Mutex<Seen>>,
}

impl PostWaterGame {
    pub fn inventory(declared: SnapshotReads, plan: Plan) -> (GamePasses, Shared) {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let game = Arc::new(Self {
            declared,
            plan,
            seen: seen.clone(),
        });
        (GamePasses::new().with(game), seen)
    }
}

fn probe() -> TargetDeclaration {
    TargetDeclaration::new(PROBE, wgpu::TextureFormat::Rgba32Float, TargetExtent::Frame)
        .with_usage(wgpu::TextureUsages::COPY_SRC)
}

impl GameFeatureFactory for PostWaterGame {
    fn name(&self) -> &'static str {
        "post-water game"
    }

    fn snapshot_reads(&self) -> SnapshotReads {
        self.declared
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.plan.replaces
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![probe()]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        let program = |body: &str, bindings: Vec<_>| {
            common::graphics(
                &runtime,
                &format!("{FULLSCREEN}{body}"),
                "vs",
                "fs",
                StageDeclarations::default(),
                StageDeclarations {
                    resources: ResourceLayout::new(bindings),
                    constants: Vec::new(),
                },
            )
        };
        let programs = Programs {
            write_left: program(WRITE_LEFT, Vec::new()),
            magenta: program(MAGENTA, Vec::new()),
            read_linear: program(
                READ_LINEAR,
                vec![texture_binding(0, "snapshot.post_linear", unfiltered)],
            ),
            read_opaque: program(
                READ_OPAQUE,
                vec![
                    texture_binding(0, "snapshot.opaque_device", unfiltered),
                    texture_binding(1, "snapshot.opaque_linear", unfiltered),
                ],
            ),
            read_all: program(
                READ_ALL,
                vec![
                    texture_binding(0, "snapshot.post_device", unfiltered),
                    texture_binding(1, "snapshot.post_linear", unfiltered),
                    texture_binding(2, "snapshot.opaque_device", unfiltered),
                    texture_binding(3, "snapshot.opaque_linear", unfiltered),
                ],
            ),
        };
        Ok(Box::new(PostWaterFeature {
            game: self.clone(),
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            snapshots: context.snapshots.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                programs,
                draws: Vec::new(),
            })),
        }))
    }
}

struct Programs {
    write_left: GraphicsProgram,
    read_opaque: GraphicsProgram,
    magenta: GraphicsProgram,
    read_linear: GraphicsProgram,
    read_all: GraphicsProgram,
}

struct Parts {
    runtime: ProgramRuntime,
    programs: Programs,
    draws: Vec<(PassEntry, PreparedProgramDraw)>,
}

struct PostWaterFeature {
    game: PostWaterGame,
    targets: GameTargets,
    camera: worldrunner_render_scene::compositor::FrameCameraHandle,
    snapshots: FrameSnapshotHandle,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for PostWaterFeature {
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
        let plan = self.game.plan;
        let mut declared = Vec::new();
        let mut add = |spec: GamePassSpec| -> Result<(), FrameError> {
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
            Ok(())
        };
        if plan.write_depth {
            add(GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
                .writing([ColorOutput::Frame])
                .with_depth(DepthAccess::ReadWrite))?;
        }
        if plan.early_reader {
            add(GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 1)
                .writing([ColorOutput::Frame])
                .reading_snapshots(Reading::Linear.reads()))?;
        }
        if let Some(reading) = plan.reader {
            add(GamePassSpec::new(GamePlace::AfterTransparent, 0)
                .writing([ColorOutput::Target(probe().layer(0))])
                .reading_snapshots(reading.reads()))?;
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(PostWaterFrame {
            game: self.game.clone(),
            camera: self.camera.clone(),
            snapshots: self.snapshots.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct PostWaterFrame {
    game: PostWaterGame,
    camera: worldrunner_render_scene::compositor::FrameCameraHandle,
    snapshots: FrameSnapshotHandle,
    parts: Arc<Mutex<Parts>>,
}

fn sampled(semantic: &str, image: Option<&SnapshotImage>) -> Result<Import, FrameError> {
    let image = image.ok_or_else(|| FrameError::FeaturePreparation {
        feature: "post-water game",
        reason: format!("the frame lent no {semantic}"),
    })?;
    Ok(Import::texture(
        EngineSemantic::named(semantic),
        image.texture.clone(),
        image.view.clone(),
    ))
}

impl PostWaterFrame {
    fn imports(reading: Reading, lent: &FrameSnapshots) -> Result<Vec<Import>, FrameError> {
        if reading == Reading::Opaque {
            return Ok(vec![
                sampled("snapshot.opaque_device", lent.device_depth.as_ref())?,
                sampled("snapshot.opaque_linear", lent.linear_depth.as_ref())?,
            ]);
        }
        let post_linear = sampled("snapshot.post_linear", lent.post_water_depth.as_ref())?;
        Ok(match reading {
            Reading::Linear | Reading::Opaque => vec![post_linear],
            Reading::All => vec![
                sampled(
                    "snapshot.post_device",
                    lent.post_water_device_depth.as_ref(),
                )?,
                post_linear,
                sampled("snapshot.opaque_device", lent.device_depth.as_ref())?,
                sampled("snapshot.opaque_linear", lent.linear_depth.as_ref())?,
            ],
        })
    }

    fn prepare_entry(
        &self,
        context: &PrepareContext<'_>,
        entry: PassEntry,
        reading: Option<Reading>,
    ) -> Result<(), FrameError> {
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        let target = PassTarget::of(context, entry)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let imports = match reading {
            Some(reading) => Self::imports(reading, &self.snapshots.current()?)?,
            None => Vec::new(),
        };
        let program = match (entry, reading) {
            (WRITER, _) => &parts.programs.write_left,
            (READER, Some(Reading::Linear)) => &parts.programs.read_linear,
            (READER, Some(Reading::Opaque)) => &parts.programs.read_opaque,
            (READER, _) => &parts.programs.read_all,
            _ => &parts.programs.magenta,
        };
        let draw = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports: &imports,
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                target.signature().unwrap(),
            )
            .map_err(|error| common::program_error("post-water game", &error))?;
        target.admit("post-water game", &draw)?;
        parts.draws.push((entry, draw));
        Ok(())
    }
}

impl PreparedFeature for PostWaterFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.parts.lock().unwrap().draws.clear();
        let lent = self.snapshots.current();
        self.game.seen.lock().unwrap().lent.push(
            lent.as_ref()
                .map(|lent| Lent {
                    linear_depth: lent.linear_depth.is_some(),
                    device_depth: lent.device_depth.is_some(),
                    post_water_depth: lent.post_water_depth.is_some(),
                    post_water_device_depth: lent.post_water_device_depth.is_some(),
                })
                .map_err(|error| *error),
        );
        let plan = self.game.plan;
        if plan.write_depth {
            self.prepare_entry(context, WRITER, None)?;
        }
        if plan.early_reader {
            self.prepare_entry(context, EARLY_READER, None)?;
        }
        if let Some(reading) = plan.reader {
            self.prepare_entry(context, READER, Some(reading))?;
        }
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        let (_, draw) = parts
            .draws
            .iter()
            .find(|(entry, _)| *entry == context.entry)
            .expect("prepared");
        draw.record(pass);
        self.game.seen.lock().unwrap().recorded.push(context.entry);
    }
}
