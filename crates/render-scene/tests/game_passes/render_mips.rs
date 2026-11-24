//! A game that builds a pyramid in a renderable target of four mips, one pass per mip: pass 0
//! fills mip 0 with a known pattern, and pass m samples mip m-1 of the same texture and writes mip
//! m as the average or the minimum of each 2x2 block. A digest pass then reads the whole chain.
//! Each mip of the target is written, versioned and kept on its own, so a frame may rewrite some
//! mips over the ones an earlier frame left.
use super::common::{self, texture_array_binding, uniform_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use super::pyramid_model::{
    DIGEST, EXTENT, FORMAT, LAYER, MIPS, Model, PYRAMID, Reduce, assert_digest, assert_pyramid,
};
use super::shadowed;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{EngineSemantic, ResourceLayout};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, PreparedProgramDraw, ProgramRuntime,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameCameraHandle, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses,
        GameTargets, PassTarget, TargetDeclaration, TargetExtent,
    },
    wgpu,
};

const DIGEST_OCCURRENCE: u32 = 10;

const COMMON_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}";

/// Mip 0: the uploaded base plus the texel's column and 16 times its row.
const SEED_WGSL: &str = "
@group(0) @binding(0) var<uniform> constants: vec4<f32>;
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<f32>(floor(at.xy));
    return vec4<f32>(constants.x + p.x + 16.0 * p.y, 0.0, 0.0, 1.0);
}";

/// Mip m of layer 1: the average (mode 0) or the minimum (mode 1) of the 2x2 block of the source mip.
const REDUCE_WGSL: &str = "
@group(0) @binding(0) var source: texture_2d_array<f32>;
@group(0) @binding(1) var<uniform> constants: vec4<f32>;
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(at.xy)) * 2;
    let a = textureLoad(source, p, 1, 0).r;
    let b = textureLoad(source, p + vec2<i32>(1, 0), 1, 0).r;
    let c = textureLoad(source, p + vec2<i32>(0, 1), 1, 0).r;
    let d = textureLoad(source, p + vec2<i32>(1, 1), 1, 0).r;
    let low = min(min(a, b), min(c, d));
    let mean = (a + b + c + d) * 0.25;
    return vec4<f32>(select(mean, low, constants.y > 0.5), 0.0, 0.0, 1.0);
}";

/// Texel i is the probe texel of mip i of layer 1 of the whole chain, bound with every mip.
const DIGEST_WGSL: &str = "
@group(0) @binding(0) var chain: texture_2d_array<f32>;
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let level = i32(floor(at.x));
    return vec4<f32>(textureLoad(chain, vec2<i32>(1, 1), 1, level).r, 0.0, 0.0, 1.0);
}";

/// What the next frame does wrong, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    /// Renders into mip 4 of a four-mip target.
    WritePastChain,
    /// The pass of mip 1 also samples mip 1.
    ReadWhatItWrites,
    /// The pass of mip 1 samples every mip of the layer, which includes the one it writes.
    ReadTheWholeChainWhileWriting,
    /// The pass of mip 1 asks for a sampled view of mips past the chain.
    BindPastChain,
}

pub struct Knobs {
    /// Mips the frame writes, in pass order, each pass writing its own mip.
    pub write: Vec<u32>,
    /// The base the seed pass adds to its pattern.
    pub base: f32,
    pub reduce: Reduce,
    /// Whether a pass of the frame reads the whole chain into the digest target.
    pub digest: bool,
    pub persistent: bool,
    pub fault: Fault,
    pub prepares: u32,
    /// The extent each pass of the last frame reported for its attachments, by pass entry.
    pub extents: BTreeMap<u32, Option<[u32; 2]>>,
}

pub type Shared = Arc<Mutex<Knobs>>;

pub fn knobs(persistent: bool) -> Shared {
    Arc::new(Mutex::new(Knobs {
        write: Vec::new(),
        base: 0.0,
        reduce: Reduce::Average,
        digest: false,
        persistent,
        fault: Fault::None,
        prepares: 0,
        extents: BTreeMap::new(),
    }))
}

/// What the next frame writes.
pub fn next(knobs: &Shared, write: &[u32], base: f32, reduce: Reduce, digest: bool) {
    let mut knobs = knobs.lock().unwrap();
    knobs.write = write.to_vec();
    knobs.base = base;
    knobs.reduce = reduce;
    knobs.digest = digest;
}

pub fn declaration(persistent: bool) -> TargetDeclaration {
    let target = TargetDeclaration::new(PYRAMID, FORMAT, TargetExtent::Fixed(EXTENT))
        .layers(2)
        .mips(MIPS)
        .with_usage(wgpu::TextureUsages::COPY_SRC);
    if persistent {
        target.persistent()
    } else {
        target
    }
}

fn digest_declaration() -> TargetDeclaration {
    TargetDeclaration::new(DIGEST, FORMAT, TargetExtent::Fixed([MIPS, 1]))
        .with_usage(wgpu::TextureUsages::COPY_SRC)
}

fn pass_entry(occurrence: u32) -> PassEntry {
    PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), occurrence)
}

pub struct PyramidGame {
    pub knobs: Shared,
}

pub fn inventory(knobs: &Shared) -> GamePasses {
    GamePasses::new().with(Arc::new(PyramidGame {
        knobs: knobs.clone(),
    }))
}

pub fn rig(knobs: &Shared) -> Rig {
    let inventory = inventory(knobs);
    Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    })
}

struct Parts {
    runtime: ProgramRuntime,
    seed: GraphicsProgram,
    reduce: GraphicsProgram,
    digest: GraphicsProgram,
    /// One constants buffer per pass, so the passes of a frame keep their own values.
    constants: Vec<wgpu::Buffer>,
    draws: BTreeMap<u32, PreparedProgramDraw>,
}

impl GameFeatureFactory for PyramidGame {
    fn name(&self) -> &'static str {
        "pyramid"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            declaration(self.knobs.lock().unwrap().persistent),
            digest_declaration(),
        ]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let device = context.context.device();
        let runtime = ProgramRuntime::new(
            device.clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let program = |body: &str, resources| {
            let wgsl = format!("{COMMON_WGSL}{body}");
            common::graphics(
                &runtime,
                &wgsl,
                "vs",
                "fs",
                StageDeclarations::default(),
                StageDeclarations {
                    resources: ResourceLayout::new(resources),
                    constants: Vec::new(),
                },
            )
        };
        let plain = wgpu::TextureSampleType::Float { filterable: false };
        let seed = program(SEED_WGSL, vec![uniform_binding(0, "pyramid.constants")]);
        let reduce = program(
            REDUCE_WGSL,
            vec![
                texture_array_binding(0, "pyramid.source", plain),
                uniform_binding(1, "pyramid.constants"),
            ],
        );
        let digest = program(
            DIGEST_WGSL,
            vec![texture_array_binding(0, "pyramid.chain", plain)],
        );
        let constants = (0..MIPS)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("pyramid constants"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        Ok(Box::new(Pyramid {
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            knobs: self.knobs.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                seed,
                reduce,
                digest,
                constants,
                draws: BTreeMap::new(),
            })),
        }))
    }
}

struct Pyramid {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for Pyramid {
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
        let target = declaration(knobs.persistent);
        let mut declared = Vec::new();
        for mip in &knobs.write {
            let written = if knobs.fault == Fault::WritePastChain {
                MIPS
            } else {
                *mip
            };
            let mut spec = GamePassSpec::new(GamePlace::BeforeOpaque, *mip)
                .writing([ColorOutput::Target(target.mip(LAYER, written))]);
            if *mip > 0 {
                spec = spec.reading_mips([target.mip(LAYER, mip - 1)]);
            }
            match (*mip, knobs.fault) {
                (1, Fault::ReadWhatItWrites) => {
                    spec = spec.reading_mips([target.mip(LAYER, 1)]);
                }
                (1, Fault::ReadTheWholeChainWhileWriting) => {
                    spec = spec.reading([target.layer(LAYER)]);
                }
                _ => {}
            }
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        if knobs.digest {
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, DIGEST_OCCURRENCE)
                .writing([ColorOutput::Target(digest_declaration().layer(0))])
                .reading([target.layer(LAYER)]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(PyramidFrame {
            targets: self.targets.clone(),
            camera: self.camera.clone(),
            knobs: self.knobs.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct PyramidFrame {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl PyramidFrame {
    /// Prepares the draw of the pass `occurrence` the first time a frame has it: the seed of
    /// mip 0, the reduction of mip m over mip m-1, or the digest over every mip.
    fn prepare_draw(
        &self,
        context: &PrepareContext<'_>,
        parts: &mut Parts,
        occurrence: u32,
    ) -> Result<(), FrameError> {
        let target = PassTarget::of(context, pass_entry(occurrence))?;
        self.knobs
            .lock()
            .unwrap()
            .extents
            .insert(occurrence, target.extent());
        if parts.draws.contains_key(&occurrence) {
            return Ok(());
        }
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let constants = Import::buffer(
            EngineSemantic::named("pyramid.constants"),
            parts.constants[occurrence.min(MIPS - 1) as usize].clone(),
        );
        let pyramid = self.targets.texture(PYRAMID)?;
        let (program, imports) = if occurrence == DIGEST_OCCURRENCE {
            let chain = self.targets.sampled_view(PYRAMID)?;
            (
                &parts.digest,
                vec![Import::texture(
                    EngineSemantic::named("pyramid.chain"),
                    pyramid,
                    chain,
                )],
            )
        } else if occurrence == 0 {
            (&parts.seed, vec![constants])
        } else {
            let past = self.knobs.lock().unwrap().fault == Fault::BindPastChain;
            let source = self.targets.sampled_mips(
                PYRAMID,
                occurrence - 1..if past { MIPS + 1 } else { occurrence },
            )?;
            (
                &parts.reduce,
                vec![
                    Import::texture(EngineSemantic::named("pyramid.source"), pyramid, source),
                    constants,
                ],
            )
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
            .map_err(|error| common::program_error("pyramid", &error))?;
        target.admit("pyramid", &draw)?;
        parts.draws.insert(occurrence, draw);
        Ok(())
    }
}

impl PreparedFeature for PyramidFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let (write, base, reduce, digest) = {
            let mut knobs = self.knobs.lock().unwrap();
            knobs.prepares += 1;
            (knobs.write.clone(), knobs.base, knobs.reduce, knobs.digest)
        };
        let mut parts = self.parts.lock().unwrap();
        for mip in write {
            self.prepare_draw(context, &mut parts, mip)?;
            let mode = if reduce == Reduce::Minimum {
                1.0_f32
            } else {
                0.0
            };
            let bytes: Vec<u8> = [base, mode, 0.0, 0.0]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            context.upload_buffer(&parts.constants[mip as usize], 0, &bytes);
        }
        if digest {
            self.prepare_draw(context, &mut parts, DIGEST_OCCURRENCE)?;
        }
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        parts
            .draws
            .get(&context.entry.occurrence())
            .expect("prepared")
            .record(pass);
    }
}

fn assert_written(rig: &Rig, mips: [bool; MIPS as usize], context: &str) {
    let targets = rig.renderer.game_targets();
    for (mip, written) in (0..).zip(mips) {
        assert_eq!(
            targets.is_mip_written(PYRAMID, LAYER, mip),
            written,
            "{context}: mip {mip}"
        );
    }
    assert_eq!(
        targets.is_written(PYRAMID, LAYER),
        mips.iter().all(|written| *written),
        "{context}: the layer counts written once every mip is"
    );
    assert!(
        (0..MIPS).all(|mip| !targets.is_mip_written(PYRAMID, 0, mip)),
        "{context}: layer 0 is never written"
    );
}

#[test]
fn a_pyramid_pass_samples_the_mip_above_and_writes_its_own() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    for reduce in [Reduce::Average, Reduce::Minimum] {
        let knobs = knobs(false);
        let mut rig = rig(&knobs);
        next(&knobs, &[0, 1, 2, 3], 3.0, reduce, false);
        rig.render().unwrap();
        let mut model = Model::new();
        model.write(&[0, 1, 2, 3], 3.0, reduce);
        assert_pyramid(&rig, &model, &format!("{reduce:?}"));
        let knobs = knobs.lock().unwrap();
        assert_eq!(
            knobs.extents,
            BTreeMap::from([
                (0, Some([16, 16])),
                (1, Some([8, 8])),
                (2, Some([4, 4])),
                (3, Some([2, 2])),
            ]),
            "each pass opens the extent of the mip it writes"
        );
    }
}

#[test]
fn a_later_pass_reads_the_whole_chain() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(false);
    let mut rig = rig(&knobs);
    next(&knobs, &[0, 1, 2, 3], 5.0, Reduce::Average, true);
    rig.render().unwrap();
    let mut model = Model::new();
    model.write(&[0, 1, 2, 3], 5.0, Reduce::Average);
    assert_pyramid(&rig, &model, "frame 1");
    assert_digest(&rig, &model, "the digest of frame 1");
    let expected = model.digest();
    assert!(
        expected
            .windows(2)
            .all(|pair| pair[0].total_cmp(&pair[1]).is_ne()),
        "{expected:?}: a read of the wrong mip shows"
    );
}

/// Runs `frames` (the mips each writes, its base and whether it digests) over one renderer of a
/// persistent pyramid, drawing each frame with `render`, and checks the pyramid, the written
/// state and, when it digests, the digest after every one.
fn persistent_frames(
    mut render: impl FnMut(&mut Rig, u64),
    mut rig: Rig,
    knobs: &Shared,
    frames: &[(&[u32], f32, bool)],
) {
    let mut model = Model::new();
    let mut written = [false; MIPS as usize];
    for (frame, (mips, base, digest)) in (0_u64..).zip(frames) {
        next(knobs, mips, *base, Reduce::Minimum, *digest);
        render(&mut rig, frame);
        model.write(mips, *base, Reduce::Minimum);
        for mip in *mips {
            written[*mip as usize] = true;
        }
        let context = format!("frame {frame} writing {mips:?}");
        assert_written(&rig, written, &context);
        assert_pyramid(&rig, &model, &context);
        if *digest {
            assert_digest(&rig, &model, &context);
        }
    }
}

const FRAMES: [(&[u32], f32, bool); 6] = [
    (&[0, 1, 2, 3], 1.0, true),
    (&[2, 3], 2.0, true),
    (&[0], 7.0, true),
    (&[1], 9.0, false),
    (&[], 0.0, true),
    (&[0, 1, 2, 3], 11.0, true),
];

#[test]
fn frames_that_rewrite_some_mips_leave_the_others_as_they_were() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(true);
    let rig = rig(&knobs);
    persistent_frames(
        |rig, _| {
            rig.render().unwrap();
        },
        rig,
        &knobs,
        &FRAMES,
    );
}

#[test]
fn a_pyramid_frame_makes_each_view_of_the_chain_once() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(false);
    let mut rig = rig(&knobs);
    next(&knobs, &[0, 1, 2, 3], 1.0, Reduce::Minimum, true);
    rig.render().unwrap();
    let views = rig.renderer.game_targets().views_created();
    assert!(
        views > 0,
        "the first frame made the views it attaches and samples"
    );
    for frame in 0..4 {
        rig.render().unwrap();
        assert_eq!(
            rig.renderer.game_targets().views_created(),
            views,
            "frame {frame} after the first made a view of a target"
        );
    }
}

#[test]
fn shadowed_same_frame_captures_keep_a_pyramid_mip_by_mip() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(true);
    let (rig, mut scene) = shadowed::shadowed_with(inventory(&knobs), wgpu::Features::empty());
    persistent_frames(
        |rig, frame| {
            shadowed::capture(rig, &mut scene, frame);
        },
        rig,
        &knobs,
        &FRAMES[..3],
    );
}

/// The refusal of the first frame of a pyramid that writes `write` with `fault`, and nothing
/// submitted or kept.
fn refusal(persistent: bool, fault: Fault, write: &[u32]) -> FrameError {
    let knobs = knobs(persistent);
    next(&knobs, write, 0.0, Reduce::Average, false);
    knobs.lock().unwrap().fault = fault;
    let mut rig = rig(&knobs);
    let submitted = rig.context.submission().submitted_serial();
    let error = rig.render().unwrap_err();
    assert_eq!(
        rig.context.submission().submitted_serial(),
        submitted,
        "nothing submitted"
    );
    assert_written(&rig, [false; MIPS as usize], "after the refusal");
    error
}

#[test]
fn a_pass_that_renders_into_a_mip_past_the_chain_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    assert_eq!(
        refusal(true, Fault::WritePastChain, &[0]),
        FrameError::AttachmentView(declaration(true).mip(LAYER, MIPS))
    );
}

#[test]
fn a_read_of_a_mip_no_earlier_pass_wrote_this_frame_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    for persistent in [false, true] {
        assert_eq!(
            refusal(persistent, Fault::None, &[2, 3]),
            FrameError::MissingProducer(declaration(persistent).mip(LAYER, 1)),
            "persistent {persistent}: mip 1 holds nothing yet"
        );
    }
}

#[test]
fn a_digest_of_a_chain_with_an_unwritten_mip_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs(true);
    next(&knobs, &[0, 1, 2], 0.0, Reduce::Average, false);
    let mut rig = rig(&knobs);
    rig.render().unwrap();
    next(&knobs, &[], 0.0, Reduce::Average, true);
    assert_eq!(
        rig.render().unwrap_err(),
        FrameError::MissingProducer(declaration(true).mip(LAYER, 3)),
    );
}

#[test]
fn a_pass_cannot_sample_the_mip_it_renders_into() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let target = declaration(true);
    assert_eq!(
        refusal(true, Fault::ReadWhatItWrites, &[0, 1]),
        FrameError::AttachmentHazard(target.mip(LAYER, 1))
    );
    assert_eq!(
        refusal(true, Fault::ReadTheWholeChainWhileWriting, &[0, 1]),
        FrameError::AttachmentHazard(target.mip(LAYER, 1)),
        "a read of the layer is a read of every mip, the written one included"
    );
}

#[test]
fn a_sampled_view_of_mips_past_the_chain_is_refused_by_name_before_anything_records() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    assert_eq!(
        refusal(true, Fault::BindPastChain, &[0, 1]),
        FrameError::AttachmentView(ResourceRef {
            name: PYRAMID,
            version: 0,
            mip: MIPS,
            layer: 0,
        })
    );
}
