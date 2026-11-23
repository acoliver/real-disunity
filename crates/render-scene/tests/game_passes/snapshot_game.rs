//! A test game that samples the frame's opaque snapshots at `GamePlace::AfterOpaqueSnapshots`,
//! and the readbacks its tests share.
use super::common::{self, texture_binding};
use super::fixture::{Rig, SIZE};
use std::cell::RefCell;
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
        ColorOutput, FrameSnapshotError, FrameSnapshotHandle, FrameSnapshots, GameBuild,
        GameFeatureFactory, GamePassSpec, GamePasses, GameTargets, PassTarget, PostWaterDepth,
        SnapshotImage, SnapshotReads, StockReplacement, TargetDeclaration, TargetExtent,
    },
    wgpu,
};

const VERTEX: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}";

/// Copies the opaque colour snapshot, texel for texel, into a game target.
const COPY_COLOR: &str = "
@group(0) @binding(0) var snapshot: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(snapshot, vec2<i32>(p.xy), 0);
}";

/// Writes the device depth in red and the linear depth in green into a game target.
const COPY_DEPTH: &str = "
@group(0) @binding(0) var device_depth: texture_2d<f32>;
@group(0) @binding(1) var linear_depth: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    return vec4<f32>(textureLoad(device_depth, pixel, 0).r, textureLoad(linear_depth, pixel, 0).r, 0.0, 1.0);
}";

/// Writes the linear depth in red into a game target, binding no other snapshot.
const COPY_LINEAR: &str = "
@group(0) @binding(0) var linear_depth: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(linear_depth, vec2<i32>(p.xy), 0).r, 0.0, 0.0, 1.0);
}";

/// Writes the depth snapshots into the frame's own colour: device depth in red, a tenth of the
/// metres in green.
const DEPTH_TO_FRAME: &str = "
@group(0) @binding(0) var device_depth: texture_2d<f32>;
@group(0) @binding(1) var linear_depth: texture_2d<f32>;
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(p.xy);
    return vec4<f32>(textureLoad(device_depth, pixel, 0).r, textureLoad(linear_depth, pixel, 0).r * 0.1, 0.0, 1.0);
}";

const MAGENTA: &str = "
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0, 0.0, 1.0, 1.0); }";

/// The game target the colour snapshot is copied into.
pub const COLOR_COPY: &str = "snapshot game colour copy";
/// The game target the depth snapshots are written to.
pub const DEPTH_PROBE: &str = "snapshot game depth probe";

pub const COLOR_PASS: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 0);
pub const DEPTH_PASS: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 1);
pub const FRAME_PASS: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 2);
/// Reads only the linear depth snapshot.
pub const LINEAR_PASS: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 5);
pub const TRANSPARENT_PASS: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterTransparent), 0);
/// A fill drawn into the frame after the snapshots, ahead of [`LATE_COLOR_PASS`].
pub const FILL_PASS: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 3);
/// A colour copy that runs after [`FILL_PASS`].
pub const LATE_COLOR_PASS: PassEntry =
    PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaqueSnapshots), 4);
/// A fill drawn into the frame before the snapshots are taken.
pub const OPAQUE_FILL_PASS: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaque), 0);

thread_local! {
    static BUILT: RefCell<Option<FrameSnapshotHandle>> = const { RefCell::new(None) };
}

/// The handle the game was given by the renderer that was built last on this thread.
pub fn built_handle() -> FrameSnapshotHandle {
    BUILT.with(|built| built.borrow().clone().expect("a game was built"))
}

/// Where the game fills the frame with magenta.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Fill {
    #[default]
    Nowhere,
    /// After the transparent passes.
    AfterTransparent,
    /// After the opaque pass, which the snapshots then include.
    AfterOpaque,
    /// At [`FILL_PASS`], then a copy of the colour snapshot into [`COLOR_COPY`] at
    /// [`LATE_COLOR_PASS`].
    ThenCopyColor,
}

/// What the game plans each frame. The reads the factory declares are separate: a pass may read
/// only what its factory declared.
#[derive(Clone, Copy, Default)]
pub struct Plan {
    /// Copies the colour snapshot into [`COLOR_COPY`].
    pub copy_color: bool,
    /// Writes both depth snapshots into [`DEPTH_PROBE`].
    pub probe_depth: bool,
    /// Writes both depth snapshots into the frame's colour.
    pub depth_to_frame: bool,
    /// Writes only the linear depth snapshot into [`DEPTH_PROBE`], in red.
    pub probe_linear: bool,
    /// A magenta fill into the frame, and where it is drawn.
    pub fill: Fill,
    /// The stock surfaces the game draws. The factory declares it once, when it is installed, so
    /// a later `replan` does not change it.
    pub replaces: StockReplacement,
}

/// What the game saw, in the order it saw it.
#[derive(Default)]
pub struct Seen {
    /// The entry of every pass the game recorded into.
    pub recorded: Vec<PassEntry>,
    /// What the handle answered in each frame's `prepare`.
    pub lent: Vec<Result<(), FrameSnapshotError>>,
    /// Which snapshots each frame's lent set held, none when the handle refused.
    pub lent_shape: Vec<SnapshotReads>,
    /// When set, the plan every later frame uses in place of the factory's, so a test can change
    /// what the game plans between frames.
    pub replan: Option<Plan>,
}

#[derive(Clone)]
pub struct SnapGame {
    pub declared: SnapshotReads,
    pub plan: Plan,
    pub seen: Arc<Mutex<Seen>>,
}

impl SnapGame {
    /// The plan for the frame now being planned or prepared.
    fn plan_now(&self) -> Plan {
        self.seen.lock().unwrap().replan.unwrap_or(self.plan)
    }

    pub fn new(declared: SnapshotReads, plan: Plan) -> (Arc<Self>, Arc<Mutex<Seen>>) {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let game = Arc::new(Self {
            declared,
            plan,
            seen: seen.clone(),
        });
        (game, seen)
    }

    pub fn inventory(declared: SnapshotReads, plan: Plan) -> (GamePasses, Arc<Mutex<Seen>>) {
        let (game, seen) = Self::new(declared, plan);
        (GamePasses::new().with(game), seen)
    }
}

fn declarations() -> [TargetDeclaration; 2] {
    [
        TargetDeclaration::new(
            COLOR_COPY,
            wgpu::TextureFormat::Rgba16Float,
            TargetExtent::Frame,
        )
        .with_usage(wgpu::TextureUsages::COPY_SRC),
        TargetDeclaration::new(
            DEPTH_PROBE,
            wgpu::TextureFormat::Rgba32Float,
            TargetExtent::Frame,
        )
        .with_usage(wgpu::TextureUsages::COPY_SRC),
    ]
}

impl GameFeatureFactory for SnapGame {
    fn name(&self) -> &'static str {
        "snapshot game"
    }

    fn snapshot_reads(&self) -> SnapshotReads {
        self.declared
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.plan.replaces
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        declarations().to_vec()
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        BUILT.with(|built| *built.borrow_mut() = Some(context.snapshots.clone()));
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        let program = |body: &str, bindings: Vec<_>| {
            common::graphics(
                &runtime,
                &format!("{VERTEX}{body}"),
                "vs",
                "fs",
                StageDeclarations::default(),
                StageDeclarations {
                    resources: ResourceLayout::new(bindings),
                    constants: Vec::new(),
                },
            )
        };
        let depth_bindings = || {
            vec![
                texture_binding(0, "snapshot.device_depth", unfiltered),
                texture_binding(1, "snapshot.linear_depth", unfiltered),
            ]
        };
        let programs = Programs {
            copy_color: program(
                COPY_COLOR,
                vec![texture_binding(0, "snapshot.color", unfiltered)],
            ),
            copy_depth: program(COPY_DEPTH, depth_bindings()),
            copy_linear: program(
                COPY_LINEAR,
                vec![texture_binding(0, "snapshot.linear_depth", unfiltered)],
            ),
            depth_to_frame: program(DEPTH_TO_FRAME, depth_bindings()),
            magenta: program(MAGENTA, Vec::new()),
        };
        Ok(Box::new(SnapFeature {
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
    copy_color: GraphicsProgram,
    copy_depth: GraphicsProgram,
    copy_linear: GraphicsProgram,
    depth_to_frame: GraphicsProgram,
    magenta: GraphicsProgram,
}

struct Parts {
    runtime: ProgramRuntime,
    programs: Programs,
    draws: Vec<(PassEntry, PreparedProgramDraw)>,
}

struct SnapFeature {
    game: SnapGame,
    targets: GameTargets,
    camera: worldrunner_render_scene::compositor::FrameCameraHandle,
    snapshots: FrameSnapshotHandle,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for SnapFeature {
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
        let [copy, probe] = declarations();
        let plan = self.game.plan_now();
        let mut declared = Vec::new();
        let mut add = |spec: GamePassSpec| -> Result<(), FrameError> {
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
            Ok(())
        };
        let color = SnapshotReads {
            color: true,
            ..SnapshotReads::NONE
        };
        let depths = SnapshotReads {
            linear_depth: true,
            device_depth: true,
            ..SnapshotReads::NONE
        };
        let linear = SnapshotReads {
            linear_depth: true,
            ..SnapshotReads::NONE
        };
        let after = GamePlace::AfterOpaqueSnapshots;
        if plan.copy_color {
            add(GamePassSpec::new(after, 0)
                .writing([ColorOutput::Target(copy.layer(0))])
                .reading_snapshots(color))?;
        }
        if plan.probe_depth {
            add(GamePassSpec::new(after, 1)
                .writing([ColorOutput::Target(probe.layer(0))])
                .reading_snapshots(depths))?;
        }
        if plan.probe_linear {
            add(GamePassSpec::new(after, 5)
                .writing([ColorOutput::Target(probe.layer(0))])
                .reading_snapshots(linear))?;
        }
        if plan.depth_to_frame {
            add(GamePassSpec::new(after, 2)
                .writing([ColorOutput::Frame])
                .reading_snapshots(depths))?;
        }
        if plan.fill == Fill::AfterOpaque {
            add(GamePassSpec::new(GamePlace::AfterOpaque, 0).writing([ColorOutput::Frame]))?;
        }
        if plan.fill == Fill::ThenCopyColor {
            add(GamePassSpec::new(after, 3).writing([ColorOutput::Frame]))?;
            add(GamePassSpec::new(after, 4)
                .writing([ColorOutput::Target(copy.layer(0))])
                .reading_snapshots(color))?;
        }
        if plan.fill == Fill::AfterTransparent {
            add(GamePassSpec::new(GamePlace::AfterTransparent, 0).writing([ColorOutput::Frame]))?;
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(SnapFrame {
            game: self.game.clone(),
            camera: self.camera.clone(),
            snapshots: self.snapshots.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct SnapFrame {
    game: SnapGame,
    camera: worldrunner_render_scene::compositor::FrameCameraHandle,
    snapshots: FrameSnapshotHandle,
    parts: Arc<Mutex<Parts>>,
}

fn sampled(semantic: &str, image: &SnapshotImage) -> Import {
    Import::texture(
        EngineSemantic::named(semantic),
        image.texture.clone(),
        image.view.clone(),
    )
}

impl SnapFrame {
    fn imports(entry: PassEntry, lent: &FrameSnapshots) -> Result<Vec<Import>, FrameError> {
        let missing = || FrameError::FeaturePreparation {
            feature: "snapshot game",
            reason: format!("the frame lent no snapshot for {entry}"),
        };
        Ok(if entry == COLOR_PASS || entry == LATE_COLOR_PASS {
            vec![sampled(
                "snapshot.color",
                lent.color.as_ref().ok_or_else(missing)?,
            )]
        } else if entry == LINEAR_PASS {
            vec![sampled(
                "snapshot.linear_depth",
                lent.linear_depth.as_ref().ok_or_else(missing)?,
            )]
        } else if entry == DEPTH_PASS || entry == FRAME_PASS {
            vec![
                sampled(
                    "snapshot.device_depth",
                    lent.device_depth.as_ref().ok_or_else(missing)?,
                ),
                sampled(
                    "snapshot.linear_depth",
                    lent.linear_depth.as_ref().ok_or_else(missing)?,
                ),
            ]
        } else {
            Vec::new()
        })
    }

    fn prepare_entry(
        &self,
        context: &PrepareContext<'_>,
        entry: PassEntry,
        lent: &Result<FrameSnapshots, FrameSnapshotError>,
    ) -> Result<(), FrameError> {
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        let target = PassTarget::of(context, entry)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let imports = match (entry, lent) {
            (COLOR_PASS | LATE_COLOR_PASS | DEPTH_PASS | FRAME_PASS | LINEAR_PASS, Ok(lent)) => {
                Self::imports(entry, lent)?
            }
            (COLOR_PASS | LATE_COLOR_PASS | DEPTH_PASS | FRAME_PASS | LINEAR_PASS, Err(error)) => {
                return Err((*error).into());
            }
            _ => Vec::new(),
        };
        let program = match entry {
            COLOR_PASS | LATE_COLOR_PASS => &parts.programs.copy_color,
            DEPTH_PASS => &parts.programs.copy_depth,
            LINEAR_PASS => &parts.programs.copy_linear,
            FRAME_PASS => &parts.programs.depth_to_frame,
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
            .map_err(|error| common::program_error("snapshot game", &error))?;
        target.admit("snapshot game", &draw)?;
        parts.draws.push((entry, draw));
        Ok(())
    }
}

impl PreparedFeature for SnapFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.parts.lock().unwrap().draws.clear();
        let lent = self.snapshots.current();
        let plan = self.game.plan_now();
        {
            let mut seen = self.game.seen.lock().unwrap();
            seen.lent
                .push(lent.as_ref().map(|_| ()).map_err(|error| *error));
            seen.lent_shape.push(
                lent.as_ref()
                    .map_or(SnapshotReads::NONE, |lent| SnapshotReads {
                        color: lent.color.is_some(),
                        linear_depth: lent.linear_depth.is_some(),
                        device_depth: lent.device_depth.is_some(),
                        post_water_depth: match (
                            lent.post_water_depth.is_some(),
                            lent.post_water_device_depth.is_some(),
                        ) {
                            (true, true) => PostWaterDepth::LinearAndDevice,
                            (true, false) => PostWaterDepth::Linear,
                            _ => PostWaterDepth::Unread,
                        },
                    }),
            );
        }
        for (planned, entry) in [
            (plan.copy_color, COLOR_PASS),
            (plan.probe_depth, DEPTH_PASS),
            (plan.depth_to_frame, FRAME_PASS),
            (plan.probe_linear, LINEAR_PASS),
            (plan.fill == Fill::AfterOpaque, OPAQUE_FILL_PASS),
            (plan.fill == Fill::ThenCopyColor, FILL_PASS),
            (plan.fill == Fill::ThenCopyColor, LATE_COLOR_PASS),
            (plan.fill == Fill::AfterTransparent, TRANSPARENT_PASS),
        ] {
            if planned {
                self.prepare_entry(context, entry, &lent)?;
            }
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

/// The bytes of a whole texture, after every submitted frame completed. Rows are padded to the
/// copy alignment only when the texture is narrower than it, which the fixture's are not.
pub fn read_texels(rig: &Rig, texture: &wgpu::Texture, bytes_per_texel: u32) -> Vec<u8> {
    let device = rig.context.device();
    let row = SIZE[0] * bytes_per_texel;
    assert_eq!(row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: u64::from(row) * u64::from(SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE[1]),
            },
        },
        wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
    );
    rig.context
        .submission()
        .submit_commands([encoder.finish()])
        .unwrap();
    rig.read_buffer(&buffer)
}

/// One texel of a four-channel readback, as floats.
pub fn texel(bytes: &[u8], bytes_per_channel: usize, column: usize, row: usize) -> [f32; 4] {
    let index = row * SIZE[0] as usize + column;
    std::array::from_fn(|channel| {
        let at = (index * 4 + channel) * bytes_per_channel;
        match bytes_per_channel {
            4 => f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
            2 => half_to_f32(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())),
            _ => unreachable!("float texels are two or four bytes a channel"),
        }
    })
}

/// IEEE binary16 to binary32, for the finite values the frames hold.
fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    match exponent {
        0 => sign * fraction * 2f32.powi(-24),
        0x1f => f32::NAN,
        _ => sign * (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
}
