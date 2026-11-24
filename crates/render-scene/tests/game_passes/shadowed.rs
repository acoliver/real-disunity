//! A game's passes in a frame whose shadows are fitted by a same-frame capture. The capture
//! submits the whole frame as its suffix, so what the game writes, a per-frame target and a
//! persistent one, has to be part of the capture's write contract in every frame, including the
//! ones that read persistent contents an earlier frame left.
use super::common::{self, texture_binding};
use super::fixture::{Rig, SCENE_FORMAT, SIZE, quad_scene, view};
use super::prepared;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    RenderScene, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading,
    SceneDirection, SceneEffects, SceneEffectsParts, SceneShadowCascades, SceneSky, SceneSun,
};
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
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::shadows::ShadowedTarget;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameCameraHandle, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses,
        GameTargets, PassTarget, TargetDeclaration, TargetExtent,
    },
    shadows::{
        DirectionalShadowSettings, ShadowedScene,
        sdsm::{AdoptedFit, AdoptionPolicy, RangeFit, SdsmSettings},
    },
    wgpu,
};
use worldrunner_render_shadow::{MinimumSpanPolicy, SplitPolicy};

const FILL_GREEN_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }";

const FILL_RED_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }";

/// Writes the frame's colour from the red of the persistent target and the green of the
/// per-frame one, so a pixel shows exactly which of them the frame could read.
const READ_WGSL: &str = "
@group(0) @binding(0) var transient_target: texture_2d<f32>;
@group(0) @binding(1) var kept_target: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let t = textureLoad(transient_target, vec2<i32>(0, 0), 0);
    let k = textureLoad(kept_target, vec2<i32>(0, 0), 0);
    return vec4<f32>(k.r, t.g, 0.0, 1.0);
}";

const TRANSIENT: &str = "painter transient";
const KEPT: &str = "painter kept";
const FILL_TRANSIENT: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
const FILL_KEPT: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 1);
const READER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 10);

/// Which of the game's producers the next frame plans.
#[derive(Clone, Copy)]
struct Knobs {
    fill_transient: bool,
    fill_kept: bool,
}

type Shared = Arc<Mutex<Knobs>>;

fn declarations() -> [TargetDeclaration; 2] {
    [
        TargetDeclaration::new(
            TRANSIENT,
            wgpu::TextureFormat::Rgba8Unorm,
            TargetExtent::Fixed([4, 4]),
        ),
        TargetDeclaration::new(
            KEPT,
            wgpu::TextureFormat::Rgba8Unorm,
            TargetExtent::Fixed([4, 4]),
        )
        .persistent(),
    ]
}

/// A game that fills a per-frame target green and a persistent target red, then writes the
/// frame's colour from both.
struct Painter {
    knobs: Shared,
}

impl GameFeatureFactory for Painter {
    fn name(&self) -> &'static str {
        "painter"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        declarations().to_vec()
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let program = |wgsl, declarations| {
            common::graphics(
                &runtime,
                wgsl,
                "vs",
                "fs",
                StageDeclarations::default(),
                declarations,
            )
        };
        let nothing = StageDeclarations::default;
        let filter = wgpu::TextureSampleType::Float { filterable: true };
        let fill_green = program(FILL_GREEN_WGSL, nothing());
        let fill_red = program(FILL_RED_WGSL, nothing());
        let reader = program(
            READ_WGSL,
            StageDeclarations {
                resources: ResourceLayout::new(vec![
                    texture_binding(0, "painter.transient", filter),
                    texture_binding(1, "painter.kept", filter),
                ]),
                constants: Vec::new(),
            },
        );
        Ok(Box::new(PainterFeature {
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            knobs: self.knobs.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                green: fill_green,
                red: fill_red,
                read: reader,
                draws: Vec::new(),
            })),
        }))
    }
}

struct Parts {
    runtime: ProgramRuntime,
    green: GraphicsProgram,
    red: GraphicsProgram,
    read: GraphicsProgram,
    draws: Vec<(PassEntry, PreparedProgramDraw)>,
}

struct PainterFeature {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for PainterFeature {
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
        let knobs = *self.knobs.lock().unwrap();
        let [transient, kept] = declarations();
        let mut declared = Vec::new();
        let mut plan = |spec: &GamePassSpec| -> Result<(), FrameError> {
            declared.push(self.targets.plan_pass(spec, passes, views)?);
            Ok(())
        };
        if knobs.fill_transient {
            plan(
                &GamePassSpec::new(GamePlace::BeforeOpaque, 0)
                    .writing([ColorOutput::Target(transient.layer(0))]),
            )?;
        }
        if knobs.fill_kept {
            plan(
                &GamePassSpec::new(GamePlace::BeforeOpaque, 1)
                    .writing([ColorOutput::Target(kept.layer(0))]),
            )?;
        }
        plan(
            &GamePassSpec::new(GamePlace::BeforeOpaque, 10)
                .writing([ColorOutput::Frame])
                .reading([transient.layer(0), kept.layer(0)]),
        )?;
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(PainterFrame {
            targets: self.targets.clone(),
            camera: self.camera.clone(),
            knobs: self.knobs.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct PainterFrame {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl PainterFrame {
    /// Prepares the draw of an entry the first time a frame has it.
    fn prepare_entry(
        &self,
        context: &PrepareContext<'_>,
        entry: PassEntry,
    ) -> Result<(), FrameError> {
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        if parts.draws.iter().any(|(known, _)| *known == entry) {
            return Ok(());
        }
        let target = PassTarget::of(context, entry)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let imports = if entry == READER {
            let sampled = |name: &'static str, semantic: &str| -> Result<Import, FrameError> {
                let texture = self.targets.texture(name)?;
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                Ok(Import::texture(
                    EngineSemantic::named(semantic),
                    texture,
                    view,
                ))
            };
            vec![
                sampled(TRANSIENT, "painter.transient")?,
                sampled(KEPT, "painter.kept")?,
            ]
        } else {
            Vec::new()
        };
        let program = match entry {
            READER => &parts.read,
            FILL_TRANSIENT => &parts.green,
            _ => &parts.red,
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
            .map_err(|error| common::program_error("painter", &error))?;
        target.admit("painter", &draw)?;
        parts.draws.push((entry, draw));
        Ok(())
    }
}

impl PreparedFeature for PainterFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let knobs = *self.knobs.lock().unwrap();
        for (planned, entry) in [
            (knobs.fill_transient, FILL_TRANSIENT),
            (knobs.fill_kept, FILL_KEPT),
            (true, READER),
        ] {
            if planned {
                self.prepare_entry(context, entry)?;
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
    }
}

fn inventory() -> (GamePasses, Shared) {
    let knobs = Arc::new(Mutex::new(Knobs {
        fill_transient: true,
        fill_kept: true,
    }));
    let inventory = GamePasses::new().with(Arc::new(Painter {
        knobs: knobs.clone(),
    }));
    (inventory, knobs)
}

/// The first frame writes both targets. Later frames write the per-frame target again and leave
/// the persistent one as the first frame did.
fn first_frame(knobs: &Shared) {
    *knobs.lock().unwrap() = Knobs {
        fill_transient: true,
        fill_kept: true,
    };
}

fn later_frame(knobs: &Shared) {
    *knobs.lock().unwrap() = Knobs {
        fill_transient: true,
        fill_kept: false,
    };
}

/// A pixel outside the stock quad shows the game's colour: the persistent red and the
/// per-frame green.
fn assert_painted(image: &[[u8; 4]], context: &str) {
    assert_eq!(Rig::pixel(image, 3, 20), [255, 255, 0, 255], "{context}");
}

pub fn effects() -> SceneEffects {
    effects_with(None)
}

/// The sunlit effects of the shadowed scene, with `ssao`.
pub(super) fn effects_with(ssao: Option<worldrunner_render_plan::SceneSsao>) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: SceneSun {
            direction: SceneDirection::new([600_000, 0, 800_000]).unwrap(),
            irradiance_micro: 1_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 100_000,
            ground_irradiance_micro: 100_000,
        },
        shadows: SceneShadowCascades::new(vec![4_000], 0).unwrap(),
        ssao,
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 20_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: SceneAntiAlias::Disabled,
    })
    .unwrap()
}

/// The stock quad scene under a sun, which the shadowed scene fits cascades to.
pub(super) fn lit_scene() -> RenderScene {
    let source = quad_scene();
    source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects()))
        .unwrap()
}

/// A rig whose renderer carries the game, and the shadowed scene that captures its frames.
pub(super) fn shadowed(inventory: GamePasses) -> (Rig, ShadowedScene) {
    shadowed_with(inventory, wgpu::Features::empty())
}

/// As [`shadowed`], on a device with `features`.
pub fn shadowed_with(inventory: GamePasses, features: wgpu::Features) -> (Rig, ShadowedScene) {
    let rig = Rig::with_features(&quad_scene(), SIZE, features, |settings| {
        settings.game_passes = inventory;
    });
    let scene = fitted(&rig, &lit_scene(), SCENE_FORMAT);
    (rig, scene)
}

/// The shadowed scene that captures `rig`'s frames of `source` in `format`, fitting its cascades
/// to the depth of the same frame.
pub(super) fn fitted(
    rig: &Rig,
    source: &RenderScene,
    format: wgpu::TextureFormat,
) -> ShadowedScene {
    let mut scene = ShadowedScene::new(
        &rig.context,
        PreparedScene::new(source).unwrap(),
        DirectionalShadowSettings {
            focus_meters: glam::Vec3::new(0.0, 0.0, 0.5),
            half_extent_meters: 4.0,
            depth_half_extent_meters: 4.0,
            resolution: 512,
            depth_bias: 0.001,
            normal_bias_meters: 0.01,
            refresh_policy: worldrunner_render_scene::shadows::CascadeRefreshPolicy::default(),
        },
        ShadowedTarget::new(format, 1),
    )
    .unwrap();
    scene
        .set_depth_fitting(
            &rig.context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: RangeFit {
                    near_step: 0.1,
                    far_step: 1.0,
                    minimum_span: 1.0,
                    minimum_span_policy: MinimumSpanPolicy::ExtendNear,
                    active_maximum: None,
                    split_lambda: 0.5,
                    split_policy: SplitPolicy::Practical,
                },
            }),
        )
        .unwrap();
    scene
}

/// Captures one frame with the rig's renderer and returns its output, which shows the scene
/// colour unchanged.
pub(super) fn capture(rig: &mut Rig, scene: &mut ShadowedScene, frame: u64) -> Vec<[u8; 4]> {
    capture_fit(rig, scene, frame);
    rig.output()
}

/// Captures one frame with the rig's renderer and returns the fit it adopted.
pub(super) fn capture_fit(rig: &mut Rig, scene: &mut ShadowedScene, frame: u64) -> AdoptedFit {
    let targets = rig.renderer.targets();
    let color =
        AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture()).unwrap();
    let depth =
        AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture()).unwrap();
    let fit = rig
        .renderer
        .encoding()
        .render_depth_fitted(
            &rig.context,
            &view(SIZE),
            (&color, &depth),
            (Some(frame), true, &[]),
            scene,
        )
        .unwrap_or_else(|error| panic!("capture of frame {frame}: {error:?}"));
    fit.unwrap_or_else(|| panic!("frame {frame} adopted its own fit"))
}

#[test]
fn a_shadowed_same_frame_capture_writes_the_games_per_frame_and_persistent_targets() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, knobs) = inventory();
    let (mut rig, mut scene) = shadowed(inventory);
    first_frame(&knobs);
    assert_painted(&capture(&mut rig, &mut scene, 0), "frame 0");
    assert!(rig.renderer.game_targets().is_written(KEPT, 0));
}

#[test]
fn a_shadowed_capture_reads_the_persistent_target_an_earlier_capture_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, knobs) = inventory();
    let (mut rig, mut scene) = shadowed(inventory);
    first_frame(&knobs);
    assert_painted(&capture(&mut rig, &mut scene, 0), "frame 0");
    later_frame(&knobs);
    for frame in 1..4 {
        assert_painted(
            &capture(&mut rig, &mut scene, frame),
            &format!("frame {frame}"),
        );
    }
}

#[test]
fn an_unshadowed_frame_reads_the_persistent_target_an_earlier_frame_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, knobs) = inventory();
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    first_frame(&knobs);
    assert_painted(&rig.render().unwrap(), "frame 0");
    later_frame(&knobs);
    for frame in 1..4 {
        assert_painted(&rig.render().unwrap(), &format!("frame {frame}"));
    }
}

/// A rig with the block game whose frames a shadowed scene captures.
fn shadowed_blocks(knobs: &prepared::Shared) -> (Rig, ShadowedScene) {
    shadowed_with(
        prepared::inventory(knobs),
        wgpu::Features::TEXTURE_COMPRESSION_BC,
    )
}

#[test]
fn a_shadowed_capture_accepts_a_prepared_block_write() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = prepared::knobs(&[0, 1], &[0, 1]);
    let (mut rig, mut scene) = shadowed_blocks(&knobs);
    prepared::assert_blocks(&capture(&mut rig, &mut scene, 0), true, "frame 0");
    for layer in 0..2 {
        assert!(
            rig.renderer
                .game_targets()
                .is_written(prepared::BLOCKS, layer)
        );
    }
    prepared::next(&knobs, &[], &[0, 1]);
    for frame in 1..3 {
        prepared::assert_blocks(
            &capture(&mut rig, &mut scene, frame),
            true,
            &format!("frame {frame}"),
        );
    }
    assert_eq!(knobs.lock().unwrap().transfers, 2, "compressed once");
}

#[test]
fn a_shadowed_capture_compresses_one_layer_while_reading_another() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = prepared::knobs(&[0], &[0]);
    let (mut rig, mut scene) = shadowed_blocks(&knobs);
    prepared::assert_blocks(&capture(&mut rig, &mut scene, 0), false, "frame 0");
    assert!(rig.renderer.game_targets().is_written(prepared::BLOCKS, 0));
    assert!(!rig.renderer.game_targets().is_written(prepared::BLOCKS, 1));
    // Layer 0 is kept and layer 1 is written over an allocation that already holds contents.
    prepared::next(&knobs, &[1], &[0, 1]);
    prepared::assert_blocks(&capture(&mut rig, &mut scene, 1), true, "frame 1");
    assert!(rig.renderer.game_targets().is_written(prepared::BLOCKS, 1));
    prepared::next(&knobs, &[], &[0, 1]);
    prepared::assert_blocks(&capture(&mut rig, &mut scene, 2), true, "frame 2");
}

/// Captures a frame for each step of `schedule`, each a layer list to compress and one to read,
/// and checks the contents every frame samples. The kernel swaps the layers' values in every
/// odd frame, so a layer compressed again shows the rewrite and a layer kept shows it did not
/// change.
fn captured_schedule(schedule: &[(&[u32], &[u32])]) -> prepared::Shared {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = prepared::knobs(&[], &[]);
    let (mut rig, mut scene) = shadowed_blocks(&knobs);
    let mut held = [None; 2];
    for (frame, (compress, read)) in schedule.iter().enumerate() {
        prepared::next(&knobs, compress, read);
        let swapped = frame % 2 == 1;
        knobs.lock().unwrap().swapped = swapped;
        for &layer in *compress {
            held[layer as usize] = Some(prepared::VALUES[layer as usize ^ usize::from(swapped)]);
        }
        let image = capture(&mut rig, &mut scene, frame as u64);
        prepared::assert_layers(&image, held, &format!("frame {frame}"));
        for layer in 0..2 {
            assert_eq!(
                rig.renderer
                    .game_targets()
                    .is_written(prepared::BLOCKS, layer),
                held[layer as usize].is_some(),
                "frame {frame} layer {layer} written"
            );
        }
    }
    knobs
}

#[test]
fn a_shadowed_capture_accepts_a_prepared_block_write_in_every_frame() {
    let both: &[u32] = &[0, 1];
    let knobs = captured_schedule(&[(both, &[]), (both, both), (both, both), (both, both)]);
    assert_eq!(knobs.lock().unwrap().transfers, 8, "compressed every frame");
}

#[test]
fn a_shadowed_capture_rewrites_one_layer_each_frame_over_a_kept_one() {
    let knobs = captured_schedule(&[
        (&[1], &[]),
        (&[0], &[1]),
        (&[1], &[0]),
        (&[0], &[0, 1]),
        (&[0], &[0, 1]),
    ]);
    assert_eq!(knobs.lock().unwrap().transfers, 5);
}

#[test]
fn a_shadowed_capture_that_neither_reads_nor_writes_the_blocks_keeps_them() {
    let both: &[u32] = &[0, 1];
    let knobs = captured_schedule(&[(both, &[]), (&[], &[]), (&[], &[]), (&[0], &[]), (&[], &[])]);
    assert_eq!(knobs.lock().unwrap().transfers, 3);
}
