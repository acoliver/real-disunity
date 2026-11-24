//! A game that fills a mip chain while it prepares a frame: for each mip, a program runtime
//! kernel writes the mip's BC4 blocks into a scratch image and a block transfer moves them into
//! that mip of a persistent block-compressed target, and a later game pass reads every mip at an
//! explicit level. Each mip of each layer is written, versioned and kept on its own, so a pass
//! that reads a layer needs every mip of it.
use super::common::{self, array_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use super::shadowed;
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

pub const CHAIN: &str = "mips.height";
/// The red each mip's blocks decode to, in 255ths: distinct, so a read of the wrong level shows.
pub const VALUES: [u32; 3] = [200, 128, 64];
const MIPS: u32 = 3;
/// The layer the game fills and reads; layer 0 stays unwritten.
pub const LAYER: u32 = 1;
/// Mip 0 is two blocks a side, mip 1 one block, and mip 2 (two texels a side) one rounded-up
/// block.
const EXTENT: [u32; 2] = [8, 8];

/// The logical texels of `mip`.
fn mip_extent(mip: u32) -> [u32; 2] {
    EXTENT.map(|side| (side >> mip).max(1))
}

/// The BC4 blocks that cover `mip`, one scratch texel each.
fn mip_blocks(mip: u32) -> [u32; 2] {
    mip_extent(mip).map(|side| side.div_ceil(4))
}

/// Writes one BC4 block per invocation: the uploaded red as the first endpoint and, with every
/// index zero, every texel's value.
const COMPRESS_WGSL: &str = "
struct Value { red: vec4<u32> }
@group(0) @binding(0) var<uniform> value: Value;
@group(0) @binding(1) var blocks: texture_storage_2d<rg32uint, write>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(blocks, vec2<i32>(id.xy), vec4<u32>(value.red.x, 0u, 0u, 0u));
}";

/// Writes the frame's colour from the first texel of layer 1 at levels 0, 1 and 2, in red,
/// green and blue.
const READ_WGSL: &str = "
@group(0) @binding(0) var chain: texture_2d_array<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let zero = textureLoad(chain, vec2<i32>(0, 0), 1, 0);
    let one = textureLoad(chain, vec2<i32>(0, 0), 1, 1);
    let two = textureLoad(chain, vec2<i32>(0, 0), 1, 2);
    return vec4<f32>(zero.r, one.r, two.r, 1.0);
}";

/// What the next frame does wrong, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    /// Plans a write of mip 3, past the chain.
    PlanPastChain,
    /// Declares each planned mip written and transfers into the next mip.
    WrongMip,
    /// Declares each planned mip written and transfers into mip 3, past the chain.
    TransferPastChain,
}

pub struct Knobs {
    /// Mips of layer 1 the frame compresses while it prepares.
    pub compress: Vec<u32>,
    /// Whether a pass of the frame reads layer 1.
    pub read: bool,
    pub fault: Fault,
    pub prepares: u32,
    pub transfers: u32,
}

pub type Shared = Arc<Mutex<Knobs>>;

pub fn knobs(compress: &[u32], read: bool) -> Shared {
    Arc::new(Mutex::new(Knobs {
        compress: compress.to_vec(),
        read,
        fault: Fault::None,
        prepares: 0,
        transfers: 0,
    }))
}

/// Sets what the next frame compresses and whether it reads.
pub fn next(knobs: &Shared, compress: &[u32], read: bool) {
    let mut knobs = knobs.lock().unwrap();
    knobs.compress = compress.to_vec();
    knobs.read = read;
}

pub fn declaration() -> TargetDeclaration {
    TargetDeclaration::new(
        CHAIN,
        wgpu::TextureFormat::Bc4RUnorm,
        TargetExtent::Fixed(EXTENT),
    )
    .layers(2)
    .mips(MIPS)
    .persistent()
}

pub struct MipGame {
    pub knobs: Shared,
}

pub fn inventory(knobs: &Shared) -> GamePasses {
    GamePasses::new().with(Arc::new(MipGame {
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
                BindingSource::Engine(EngineSemantic::named("mips.value")),
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

impl GameFeatureFactory for MipGame {
    fn name(&self) -> &'static str {
        "mips"
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
                resources: ResourceLayout::new(vec![array_binding(0, CHAIN)]),
                constants: Vec::new(),
            },
        );
        let scratch = runtime
            .create_empty_texture(
                "mips scratch",
                TextureShape {
                    size: mip_blocks(0),
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
                label: Some("mips value"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        Ok(Box::new(MipFeature {
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

struct MipFeature {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

fn reader_spec() -> GamePassSpec {
    GamePassSpec::new(GamePlace::BeforeOpaque, 0)
        .writing([ColorOutput::Frame])
        .reading([declaration().layer(LAYER)])
}

impl FrameFeature for MipFeature {
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
        let mut writes: Vec<_> = knobs
            .compress
            .iter()
            .map(|mip| target.mip(LAYER, *mip))
            .collect();
        if knobs.fault == Fault::PlanPastChain {
            writes.push(target.mip(LAYER, MIPS));
        }
        self.targets.plan_prepared(&[], &writes, views)?;
        let mut declared = Vec::new();
        if knobs.read {
            declared.push(self.targets.plan_pass(&reader_spec(), passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(MipFrame {
            targets: self.targets.clone(),
            camera: self.camera.clone(),
            knobs: self.knobs.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct MipFrame {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

fn refused(error: &worldrunner_render_program::error::ProgramError) -> FrameError {
    common::program_error("mips", error)
}

impl MipFrame {
    /// Prepares the reader's draw over the target's sampled view, every layer and every mip, the
    /// first time a frame has it.
    fn prepare_reader(
        &self,
        context: &PrepareContext<'_>,
        parts: &mut Parts,
    ) -> Result<(), FrameError> {
        if parts.reader.is_some() {
            return Ok(());
        }
        let target = PassTarget::of(context, reader_spec().entry())?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let texture = self.targets.texture(CHAIN)?;
        let view = self.targets.sampled_view(CHAIN)?;
        let draw = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.read,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports: &[Import::texture(EngineSemantic::named(CHAIN), texture, view)],
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                target.signature().unwrap(),
            )
            .map_err(|error| refused(&error))?;
        target.admit("mips", &draw)?;
        parts.reader = Some(draw);
        Ok(())
    }

    /// Stages the kernel that encodes the blocks of `mip` (Lod `mip`), reading the red uploaded
    /// just before it.
    fn stage_dispatch(
        context: &mut PrepareContext<'_>,
        parts: &mut Parts,
        mip: u32,
    ) -> Result<(), FrameError> {
        let mut red = [0_u8; 16];
        red[..4].copy_from_slice(&VALUES[mip as usize].to_le_bytes());
        context.upload_buffer(&parts.value, 0, &red);
        let blocks = mip_blocks(mip);
        let dispatch = parts
            .runtime
            .prepare_imported_dispatch(&ImportedDispatch {
                program: &parts.compress,
                bindings: &[BoundResource::texture(0, 1, parts.scratch)],
                imports: &[Import::buffer(
                    EngineSemantic::named("mips.value"),
                    parts.value.clone(),
                )],
                groups: [blocks[0], blocks[1], 1],
                owners: &[],
            })
            .map_err(|error| refused(&error))?;
        context.record_prepared(
            "mips compress",
            PreparedAccess::default(),
            Box::new(dispatch),
        )
    }

    /// Stages the transfer of the scratch blocks into the mip the fault picks, declaring
    /// `resource` written.
    fn stage_transfer(
        &self,
        context: &mut PrepareContext<'_>,
        (parts, fault): (&Parts, Fault),
        resource: ResourceRef,
    ) -> Result<(), FrameError> {
        let mip = match fault {
            Fault::WrongMip => resource.mip + 1,
            Fault::TransferPastChain => MIPS,
            Fault::None | Fault::PlanPastChain => resource.mip,
        };
        let transfer = parts
            .runtime
            .prepare_lent_block_transfer(
                &BlockSource {
                    texture: parts.scratch,
                    mip: 0,
                    layer: 0,
                    origin: [0, 0],
                    extent: mip_blocks(mip),
                },
                &LentBlockDestination {
                    texture: self.targets.texture(CHAIN)?,
                    mip,
                    layer: resource.layer,
                    extent: mip_extent(mip),
                },
                &[],
            )
            .map_err(|error| refused(&error))?;
        context.record_prepared(
            "mips transfer",
            PreparedAccess {
                reads: &[],
                writes: &[resource],
            },
            Box::new(transfer),
        )
    }
}

impl PreparedFeature for MipFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let mut knobs = self.knobs.lock().unwrap();
        knobs.prepares += 1;
        let mut parts = self.parts.lock().unwrap();
        if knobs.read {
            self.prepare_reader(context, &mut parts)?;
        }
        let target = declaration();
        for mip in knobs.compress.clone() {
            Self::stage_dispatch(context, &mut parts, mip)?;
            self.stage_transfer(context, (&parts, knobs.fault), target.mip(LAYER, mip))?;
            knobs.transfers += 1;
        }
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        parts.reader.as_ref().expect("prepared").record(pass);
    }
}

/// A pixel outside the stock quad shows mip 0's red in red, mip 1's in green and mip 2's in
/// blue.
pub fn assert_chain(image: &[[u8; 4]], context: &str) {
    let pixel = Rig::pixel(image, 3, 20);
    for (channel, value) in VALUES.into_iter().enumerate() {
        assert_code_within(
            pixel[channel],
            encoded_steps(f64::from(value) / 255.0, true),
            0.0,
            &format!("{context} mip {channel}"),
        );
    }
    assert_eq!(pixel[3], 255, "{context} alpha");
}

fn assert_written(rig: &Rig, mips: [bool; 3], context: &str) {
    let targets = rig.renderer.game_targets();
    for (mip, written) in (0..).zip(mips) {
        assert_eq!(
            targets.is_mip_written(CHAIN, LAYER, mip),
            written,
            "{context}: mip {mip}"
        );
    }
    assert_eq!(
        targets.is_written(CHAIN, LAYER),
        mips.iter().all(|written| *written),
        "{context}: the layer counts written once every mip is"
    );
    assert!(
        (0..MIPS).all(|mip| !targets.is_mip_written(CHAIN, 0, mip)),
        "{context}: layer 0 is never written"
    );
}

#[test]
fn every_mip_compressed_in_prepare_is_read_at_its_own_level_by_a_later_pass() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1, 2], true);
    let mut rig = rig(&knobs);
    assert_written(&rig, [false; 3], "before any frame");
    assert_chain(&rig.render().unwrap(), "frame 1");
    assert_written(&rig, [true; 3], "after frame 1");
    let texture = rig.renderer.game_targets().texture(CHAIN).unwrap();
    assert_eq!(texture.mip_level_count(), MIPS);
    next(&knobs, &[], true);
    for frame in 2..4 {
        assert_chain(&rig.render().unwrap(), &format!("frame {frame}"));
    }
    assert_eq!(knobs.lock().unwrap().transfers, 3, "compressed once");
}

#[test]
fn a_shadowed_same_frame_capture_fills_and_reads_every_mip() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1, 2], true);
    let (mut rig, mut scene) =
        shadowed::shadowed_with(inventory(&knobs), wgpu::Features::TEXTURE_COMPRESSION_BC);
    assert_chain(&shadowed::capture(&mut rig, &mut scene, 0), "frame 0");
    assert_written(&rig, [true; 3], "after frame 0");
    next(&knobs, &[1], true);
    assert_chain(
        &shadowed::capture(&mut rig, &mut scene, 1),
        "frame 1: mip 1 compressed again over kept mips 0 and 2",
    );
    next(&knobs, &[], true);
    assert_chain(&shadowed::capture(&mut rig, &mut scene, 2), "frame 2");
    assert_eq!(knobs.lock().unwrap().transfers, 4);
}

#[test]
fn a_reader_of_a_layer_waits_until_every_mip_of_it_is_written() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(&[0, 1], false);
    let mut rig = rig(&knobs);
    rig.render().unwrap();
    assert_written(&rig, [true, true, false], "after frame 1");
    next(&knobs, &[], true);
    let submitted = rig.context.submission().submitted_serial();
    let error = rig.render().unwrap_err();
    assert_eq!(
        error,
        FrameError::MissingProducer(declaration().mip(LAYER, 2)),
        "a read of the layer needs its last mip"
    );
    assert_eq!(rig.context.submission().submitted_serial(), submitted);
    assert_written(&rig, [true, true, false], "after the refused frame");
    next(&knobs, &[2], true);
    assert_chain(
        &rig.render().unwrap(),
        "frame 3: mip 2 compressed, 0 and 1 kept",
    );
    assert_written(&rig, [true; 3], "after frame 3");
    next(&knobs, &[], true);
    assert_chain(&rig.render().unwrap(), "frame 4");
}

/// The refusal of the first frame of the game with `fault`, and its knobs after it.
fn refusal(fault: Fault, compress: &[u32]) -> (FrameError, Shared) {
    let knobs = knobs(compress, false);
    knobs.lock().unwrap().fault = fault;
    let mut rig = rig(&knobs);
    let submitted = rig.context.submission().submitted_serial();
    let error = rig.render().unwrap_err();
    assert_eq!(
        rig.context.submission().submitted_serial(),
        submitted,
        "nothing submitted"
    );
    assert_written(&rig, [false; 3], "after the refusal");
    (error, knobs)
}

#[test]
fn a_write_of_a_mip_past_the_chain_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = refusal(Fault::PlanPastChain, &[0]);
    assert_eq!(
        error,
        FrameError::AttachmentView(declaration().mip(LAYER, MIPS))
    );
    assert_eq!(knobs.lock().unwrap().prepares, 0, "refused at planning");
}

#[test]
fn a_transfer_into_another_mip_than_its_declared_write_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = refusal(Fault::WrongMip, &[0]);
    let target = declaration();
    assert_eq!(
        error,
        FrameError::PreparedWriteMismatch {
            label: "mips transfer",
            declared: vec![target.mip(LAYER, 0)],
            written: vec![target.mip(LAYER, 1)],
        }
    );
    assert_eq!(knobs.lock().unwrap().transfers, 0, "nothing staged");
}

#[test]
fn a_transfer_into_a_mip_past_the_chain_is_refused_by_name_before_staging() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = refusal(Fault::TransferPastChain, &[2]);
    let FrameError::FeaturePreparation { feature, reason } = error else {
        panic!("expected the runtime's refusal, got {error:?}");
    };
    assert_eq!(feature, "mips");
    assert!(
        reason.contains("destination mip 3 is outside the texture's 3 mips"),
        "{reason}"
    );
    assert_eq!(knobs.lock().unwrap().transfers, 0, "nothing staged");
}

/// Runs `frames` (the mips each compresses, and whether a pass reads the layer) as shadowed
/// same-frame captures over one renderer, checking after each which mips are written and, when
/// it reads, that every mip is sampled at its own value.
fn shadowed_frames(frames: &[(&[u32], bool)]) {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(frames[0].0, frames[0].1);
    let (mut rig, mut scene) =
        shadowed::shadowed_with(inventory(&knobs), wgpu::Features::TEXTURE_COMPRESSION_BC);
    let mut written = [false; 3];
    for (frame, (compress, read)) in (0_u64..).zip(frames) {
        next(&knobs, compress, *read);
        let image = shadowed::capture(&mut rig, &mut scene, frame);
        for mip in *compress {
            written[*mip as usize] = true;
        }
        let context = format!("frame {frame} compressing {compress:?}");
        assert_written(&rig, written, &context);
        if *read {
            assert_chain(&image, &context);
        }
    }
}

#[test]
fn a_shadowed_capture_fills_a_chain_over_frames_that_each_write_some_mips() {
    shadowed_frames(&[
        (&[0], false),
        (&[2], false),
        (&[1], true),
        (&[], true),
        (&[0, 2], true),
        (&[1], true),
        (&[], true),
    ]);
}

#[test]
fn a_shadowed_capture_fills_a_chain_from_the_smallest_mip_up() {
    shadowed_frames(&[
        (&[2], false),
        (&[], false),
        (&[1], false),
        (&[0], true),
        (&[2], true),
    ]);
}

#[test]
fn a_shadowed_capture_fills_one_mip_per_frame_and_rereads_it_in_the_frame_that_writes_it() {
    shadowed_frames(&[(&[1], false), (&[1], false), (&[0], false), (&[2], true)]);
}
