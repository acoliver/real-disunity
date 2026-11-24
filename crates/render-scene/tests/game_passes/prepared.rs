//! A game that compresses blocks while it prepares a frame: a program runtime kernel writes BC4
//! blocks into a scratch image, a block transfer moves them into a persistent block-compressed
//! target, and a later game pass of the same frame reads them. Both commands are staged on the
//! frame's preparation, so they are recorded after the constants uploaded before them and before
//! the first pass, and the target's layers are written like any attachment's: versioned by the
//! frame, kept once it is submitted, and owned by a capture that submits the frame.
use super::common::{self, array_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::tolerance_test_support::{assert_code_within, encoded_steps};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedAccess, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    BlockSource, BoundResource, ComputeProgram, GraphicsProgram, Import, ImportedDispatch,
    ImportedDraw, ImportedGeometry, LentBlockDestination, PreparedProgramDraw, ProgramRuntime,
    TextureHandle, TextureShape,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameCameraHandle, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses,
        GameTargets, PassTarget, TargetDeclaration, TargetExtent,
    },
    wgpu,
};

pub const BLOCKS: &str = "blocks.height";
/// A name no declaration makes.
pub const MISSING: &str = "blocks.missing";
/// The red each layer's blocks decode to, in 255ths.
pub const VALUES: [u32; 2] = [128, 64];
const EXTENT: [u32; 2] = [8, 8];
/// The blocks of one layer: two columns and two rows of four texels each.
const GRID: [u32; 2] = [2, 2];

/// Writes one BC4 block per texel of the scratch image: the uploaded red as both the first
/// endpoint and, with every index zero, every texel's value, and a second endpoint of zero.
const COMPRESS_WGSL: &str = "
struct Value { red: vec4<u32> }
@group(0) @binding(0) var<uniform> value: Value;
@group(0) @binding(1) var blocks: texture_storage_2d<rg32uint, write>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(blocks, vec2<i32>(id.xy), vec4<u32>(value.red.x, 0u, 0u, 0u));
}";

/// Writes the frame's colour from the first texel of each layer: layer 0 in red, layer 1 in
/// green.
const READ_WGSL: &str = "
@group(0) @binding(0) var blocks: texture_2d_array<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let first = textureLoad(blocks, vec2<i32>(0, 0), 0, 0);
    let second = textureLoad(blocks, vec2<i32>(0, 0), 1, 0);
    return vec4<f32>(first.r, second.r, 0.0, 1.0);
}";

/// What the next frame does wrong, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    /// Stages a write of layer 0, which the plan holds as a view and did not declare prepared.
    Unplanned,
    /// Stages the first planned layer twice.
    Repeated,
    /// Plans the layers in `compress` and stages none of them.
    Unstaged,
    /// Plans a write of a target no declaration makes.
    UndeclaredPlan,
    /// Stages a write of a target no declaration makes.
    UndeclaredStage,
    /// Plans a read of layer 0 by the preparation.
    PreparedRead,
    /// Plans a pass that renders into layer 0.
    ColorOutput,
    /// Stages a dispatch from `finish_draws` instead of `prepare`.
    FinishDraws,
    /// Transfers each planned layer's blocks into layer 1 and declares the planned layer written.
    WrongLayer,
}

pub struct Knobs {
    /// Layers the frame compresses while it prepares.
    pub compress: Vec<u32>,
    /// Layers the reader declares it samples.
    pub read: Vec<u32>,
    pub fault: Fault,
    /// Whether the reader's pass is planned before the preparation's layers.
    pub reader_first: bool,
    /// Whether the kernel writes each layer the other layer's red, so a rewrite shows.
    pub swapped: bool,
    pub prepares: u32,
    /// Dispatches and transfers staged so far.
    pub dispatches: u32,
    pub transfers: u32,
    /// What staging from `finish_draws` answered.
    pub finish_draws: Option<Result<(), FrameError>>,
}

pub type Shared = Arc<Mutex<Knobs>>;

pub fn knobs(compress: &[u32], read: &[u32]) -> Shared {
    Arc::new(Mutex::new(Knobs {
        compress: compress.to_vec(),
        read: read.to_vec(),
        fault: Fault::None,
        reader_first: false,
        swapped: false,
        prepares: 0,
        dispatches: 0,
        transfers: 0,
        finish_draws: None,
    }))
}

/// Sets what the next frame compresses and reads.
pub fn next(knobs: &Shared, compress: &[u32], read: &[u32]) {
    let mut knobs = knobs.lock().unwrap();
    knobs.compress = compress.to_vec();
    knobs.read = read.to_vec();
}

pub fn declaration() -> TargetDeclaration {
    TargetDeclaration::new(
        BLOCKS,
        wgpu::TextureFormat::Bc4RUnorm,
        TargetExtent::Fixed(EXTENT),
    )
    .layers(2)
    .persistent()
}

pub struct BlockGame {
    pub knobs: Shared,
}

pub fn inventory(knobs: &Shared) -> GamePasses {
    GamePasses::new().with(Arc::new(BlockGame {
        knobs: knobs.clone(),
    }))
}

/// A rig on a device with block compression whose renderer carries the game.
pub fn rig(knobs: &Shared) -> Rig {
    let inventory = inventory(knobs);
    Rig::with_features(
        &quad_scene(),
        SIZE,
        wgpu::Features::TEXTURE_COMPRESSION_BC,
        |settings| settings.game_passes = inventory,
    )
}

fn compute(runtime: &ProgramRuntime) -> ComputeProgram {
    let binding = |binding, source, kind| ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source,
        visibility: wgpu::ShaderStages::COMPUTE,
        kind,
    };
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(vec![
            binding(
                0,
                BindingSource::Engine(EngineSemantic::named("blocks.value")),
                ResourceKind::Uniform { min_size: 16 },
            ),
            binding(
                1,
                BindingSource::Unnamed,
                ResourceKind::StorageImage {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rg32Uint,
                    dimension: wgpu::TextureViewDimension::D2,
                },
            ),
        ]),
        constants: Vec::new(),
    };
    let stage = StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(COMPRESS_WGSL)),
            entry_point: "main".to_owned(),
            kind: StageKind::Compute,
        },
        declarations,
    )
    .unwrap();
    runtime.compute_program(&stage).unwrap()
}

impl GameFeatureFactory for BlockGame {
    fn name(&self) -> &'static str {
        "blocks"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![declaration()]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let mut runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let compress = compute(&runtime);
        let read = common::graphics(
            &runtime,
            READ_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: ResourceLayout::new(vec![array_binding(0, BLOCKS)]),
                constants: Vec::new(),
            },
        );
        let scratch = runtime
            .create_empty_texture(
                "blocks scratch",
                TextureShape {
                    size: GRID,
                    layers: 1,
                    mips: 1,
                    format: wgpu::TextureFormat::Rg32Uint,
                    storage: true,
                    view: wgpu::TextureViewDimension::D2,
                },
            )
            .unwrap();
        let value = context
            .context
            .device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("blocks value"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        Ok(Box::new(BlockFeature {
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            knobs: self.knobs.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                compress,
                read,
                scratch,
                value,
                reader: None,
            })),
        }))
    }
}

struct Parts {
    runtime: ProgramRuntime,
    compress: ComputeProgram,
    read: GraphicsProgram,
    scratch: TextureHandle,
    value: wgpu::Buffer,
    reader: Option<PreparedProgramDraw>,
}

struct BlockFeature {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for BlockFeature {
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
        let knobs = self.knobs.lock().unwrap();
        let target = declaration();
        let mut writes: Vec<_> = knobs.compress.iter().map(|l| target.layer(*l)).collect();
        let mut reads = Vec::new();
        match knobs.fault {
            Fault::UndeclaredPlan => writes = vec![ResourceRef::new(MISSING)],
            Fault::PreparedRead => reads.push(target.layer(0)),
            _ => {}
        }
        let reader = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
            .writing([ColorOutput::Frame])
            .reading(knobs.read.iter().map(|layer| target.layer(*layer)));
        let mut declared = Vec::new();
        if knobs.reader_first {
            declared.push(self.targets.plan_pass(&reader, passes, views)?);
        }
        self.targets.plan_prepared(&reads, &writes, views)?;
        if knobs.fault == Fault::ColorOutput {
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 1)
                .writing([ColorOutput::Target(target.layer(0))]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        if !knobs.reader_first {
            declared.push(self.targets.plan_pass(&reader, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(BlockFrame {
            targets: self.targets.clone(),
            camera: self.camera.clone(),
            knobs: self.knobs.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct BlockFrame {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

fn refused(error: &worldrunner_render_program::error::ProgramError) -> FrameError {
    common::program_error("blocks", error)
}

impl BlockFrame {
    /// Prepares the reader's draw the first time a frame has it.
    fn prepare_reader(
        &self,
        context: &PrepareContext<'_>,
        parts: &mut Parts,
    ) -> Result<(), FrameError> {
        if parts.reader.is_some() {
            return Ok(());
        }
        let entry = GamePassSpec::new(GamePlace::BeforeOpaque, 0).entry();
        let target = PassTarget::of(context, entry)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let texture = self.targets.texture(BLOCKS)?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let draw = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.read,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports: &[Import::texture(
                        EngineSemantic::named(BLOCKS),
                        texture,
                        view,
                    )],
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                target.signature().unwrap(),
            )
            .map_err(|error| refused(&error))?;
        target.admit("blocks", &draw)?;
        parts.reader = Some(draw);
        Ok(())
    }

    /// Stages the kernel that encodes `layer`'s blocks, reading the red uploaded just before it.
    fn stage_dispatch(
        context: &mut PrepareContext<'_>,
        parts: &mut Parts,
        layer: u32,
    ) -> Result<(), FrameError> {
        let mut red = [0_u8; 16];
        red[..4].copy_from_slice(&VALUES[layer as usize].to_le_bytes());
        context.upload_buffer(&parts.value, 0, &red);
        let dispatch = parts
            .runtime
            .prepare_imported_dispatch(&ImportedDispatch {
                program: &parts.compress,
                bindings: &[BoundResource::texture(0, 1, parts.scratch)],
                imports: &[Import::buffer(
                    EngineSemantic::named("blocks.value"),
                    parts.value.clone(),
                )],
                groups: [GRID[0], GRID[1], 1],
                owners: &[],
            })
            .map_err(|error| refused(&error))?;
        context.record_prepared(
            "blocks compress",
            PreparedAccess::default(),
            Box::new(dispatch),
        )
    }

    /// Stages the transfer of the scratch blocks into `resource`, or into layer 1 when the fault
    /// is `WrongLayer`, declaring `resource` written.
    fn stage_transfer(
        &self,
        context: &mut PrepareContext<'_>,
        (parts, fault): (&Parts, Fault),
        resource: ResourceRef,
    ) -> Result<(), FrameError> {
        let layer = if fault == Fault::WrongLayer {
            1
        } else {
            resource.layer
        };
        let transfer = parts
            .runtime
            .prepare_lent_block_transfer(
                &BlockSource {
                    texture: parts.scratch,
                    mip: 0,
                    layer: 0,
                    origin: [0, 0],
                    extent: GRID,
                },
                &LentBlockDestination {
                    texture: self.targets.texture(BLOCKS)?,
                    mip: 0,
                    layer,
                    extent: EXTENT,
                },
                &[],
            )
            .map_err(|error| refused(&error))?;
        context.record_prepared(
            "blocks transfer",
            PreparedAccess {
                reads: &[],
                writes: &[resource],
            },
            Box::new(transfer),
        )
    }

    /// The writes the frame stages, in order: the planned ones, or the fault's.
    fn staged(knobs: &Knobs) -> Vec<ResourceRef> {
        let target = declaration();
        let planned = knobs.compress.iter().map(|layer| target.layer(*layer));
        match knobs.fault {
            Fault::Unplanned => vec![target.layer(0)],
            Fault::Repeated => vec![target.layer(knobs.compress[0]); 2],
            Fault::Unstaged => Vec::new(),
            Fault::UndeclaredStage => vec![ResourceRef::new(MISSING)],
            _ => planned.collect(),
        }
    }
}

impl PreparedFeature for BlockFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let mut knobs = self.knobs.lock().unwrap();
        knobs.prepares += 1;
        let mut parts = self.parts.lock().unwrap();
        self.prepare_reader(context, &mut parts)?;
        for resource in Self::staged(&knobs) {
            let value = resource.layer.min(1) ^ u32::from(knobs.swapped);
            Self::stage_dispatch(context, &mut parts, value)?;
            knobs.dispatches += 1;
            self.stage_transfer(context, (&parts, knobs.fault), resource)?;
            knobs.transfers += 1;
        }
        Ok(())
    }

    fn finish_draws(&mut self, context: &mut PrepareContext<'_>) {
        let mut knobs = self.knobs.lock().unwrap();
        if knobs.fault != Fault::FinishDraws {
            return;
        }
        let mut parts = self.parts.lock().unwrap();
        knobs.finish_draws = Some(Self::stage_dispatch(context, &mut parts, 0));
    }

    fn draw(&self, _context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        parts.reader.as_ref().expect("prepared").record(pass);
    }
}

/// A pixel outside the stock quad shows layer 0's red in red and, when `both`, layer 1's in
/// green.
pub fn assert_blocks(image: &[[u8; 4]], both: bool, context: &str) {
    let pixel = Rig::pixel(image, 3, 20);
    let code = |value: u32| encoded_steps(f64::from(value) / 255.0, true);
    assert_code_within(
        pixel[0],
        code(VALUES[0]),
        0.0,
        &format!("{context} layer 0"),
    );
    if both {
        assert_code_within(
            pixel[1],
            code(VALUES[1]),
            0.0,
            &format!("{context} layer 1"),
        );
    }
    assert_eq!(pixel[3], 255, "{context} alpha");
}

/// A pixel outside the stock quad shows each layer's red, in red and green, for the layers the
/// frame holds contents of: the value of `layers[layer]`.
pub fn assert_layers(image: &[[u8; 4]], layers: [Option<u32>; 2], context: &str) {
    let pixel = Rig::pixel(image, 3, 20);
    for (channel, value) in layers.into_iter().enumerate() {
        if let Some(value) = value {
            assert_code_within(
                pixel[channel],
                encoded_steps(f64::from(value) / 255.0, true),
                0.0,
                &format!("{context} layer {channel}"),
            );
        }
    }
}

#[test]
fn a_feature_compresses_blocks_in_prepare_and_a_later_pass_reads_them() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1], &[0, 1]);
    let mut rig = rig(&knobs);
    assert!(!rig.renderer.game_targets().is_written(BLOCKS, 0));
    assert_blocks(&rig.render().unwrap(), true, "frame 1");
    for layer in 0..2 {
        assert!(rig.renderer.game_targets().is_written(BLOCKS, layer));
    }
    let knobs = knobs.lock().unwrap();
    assert_eq!((knobs.dispatches, knobs.transfers), (2, 2));
}

#[test]
fn a_compressed_layer_is_read_in_later_frames_without_compressing_again() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1], &[0, 1]);
    let mut rig = rig(&knobs);
    assert_blocks(&rig.render().unwrap(), true, "frame 1");
    next(&knobs, &[], &[0, 1]);
    for frame in 2..4 {
        assert_blocks(&rig.render().unwrap(), true, &format!("frame {frame}"));
    }
    let knobs = knobs.lock().unwrap();
    assert_eq!(
        (knobs.dispatches, knobs.transfers),
        (2, 2),
        "later frames staged nothing"
    );
    assert_eq!(knobs.prepares, 3);
}

#[test]
fn a_layer_compressed_again_in_every_frame_shows_each_rewrite() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1], &[0, 1]);
    let mut rig = rig(&knobs);
    for frame in 0..4_usize {
        let swapped = frame % 2 == 1;
        knobs.lock().unwrap().swapped = swapped;
        let value = |layer: usize| Some(VALUES[layer ^ usize::from(swapped)]);
        assert_layers(
            &rig.render().unwrap(),
            [value(0), value(1)],
            &format!("frame {frame}"),
        );
    }
    assert_eq!(knobs.lock().unwrap().transfers, 8, "compressed every frame");
}

#[test]
fn a_reader_planned_before_the_layers_it_reads_sees_them_compressed() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1], &[0, 1]);
    knobs.lock().unwrap().reader_first = true;
    let mut rig = rig(&knobs);
    assert_blocks(&rig.render().unwrap(), true, "frame 1");
    for layer in 0..2 {
        assert!(rig.renderer.game_targets().is_written(BLOCKS, layer));
    }
    next(&knobs, &[1], &[0, 1]);
    knobs.lock().unwrap().swapped = true;
    assert_layers(
        &rig.render().unwrap(),
        [Some(VALUES[0]), Some(VALUES[0])],
        "frame 2: layer 0 kept, layer 1 compressed again over its read view",
    );
    let knobs = knobs.lock().unwrap();
    assert_eq!((knobs.dispatches, knobs.transfers), (3, 3));
}
