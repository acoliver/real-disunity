//! A game's passes after the depth prepass in a shadowed same-frame capture, over several frames:
//! one pass raises the frame depth and the prepass normals, another writes a persistent game
//! target in the first frame only. The capture's write contract has to admit every write of the
//! prefix, and the persistent target has to reach later frames, whose passes read it.
use super::common::{self, texture_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use super::lighting::ssao;
use super::shadowed::{capture_fit, effects_with, shadowed};
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
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::shadows::ShadowedTarget;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, DepthAccess, FrameEnvironmentHandle, FrameOcclusionHandle,
        FrameSceneTexturesHandle, FrameShadowHandle, FrameTerrainHandle, GameBuild,
        GameFeatureFactory, GamePassSpec, GamePasses, GameTargets, PassTarget, TargetDeclaration,
        TargetExtent,
    },
    frame::metadata::FrameProduct,
    shadows::{
        CascadeRefreshPolicy, DirectionalShadowSettings, ShadowedScene,
        sdsm::{AdoptionPolicy, DepthRange, RangeFit, SdsmSettings},
    },
    wgpu,
};
use worldrunner_render_shadow::{MinimumSpanPolicy, SplitPolicy};

const KEPT: &str = "prefix kept";
const RAISE: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterDepthPrepass), 0);
const FILL: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterDepthPrepass), 1);
const READER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);

/// Two metres above the stock quad for the straight-down orthographic camera.
const ORTHOGRAPHIC_BLOCK: f32 = 0.29;

fn kept() -> TargetDeclaration {
    TargetDeclaration::new(
        KEPT,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    )
    .persistent()
}

const FILL_RED_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }";

/// The frame colour is whatever the persistent target holds.
const READ_WGSL: &str = "
@group(0) @binding(0) var kept_target: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let k = textureLoad(kept_target, vec2<i32>(0, 0), 0);
    return vec4<f32>(k.r, k.g, k.b, 1.0);
}";

fn raise_wgsl(depth: f32) -> String {
    format!(
        "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.25, -0.25), vec2<f32>(0.25, -0.25), vec2<f32>(0.25, 0.25),
        vec2<f32>(-0.25, -0.25), vec2<f32>(0.25, 0.25), vec2<f32>(-0.25, 0.25));
    return vec4<f32>(corners[i], {depth:?}, 1.0);
}}
@fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>(0.0, 0.0, 1.0, 1.0); }}"
    )
}

/// Which passes the next frame plans, and what the prefix feature saw of the lent inputs.
#[derive(Default)]
struct Seen {
    raise: bool,
    fill: bool,
    read: bool,
    lent: Vec<String>,
}

type Shared = Arc<Mutex<Seen>>;

struct Keeper {
    seen: Shared,
}

struct Parts {
    runtime: ProgramRuntime,
    programs: Vec<GraphicsProgram>,
    draws: Vec<(PassEntry, PreparedProgramDraw)>,
}

fn parts(context: &GameBuild<'_>, programs: Vec<(String, StageDeclarations)>) -> Arc<Mutex<Parts>> {
    let runtime = ProgramRuntime::new(
        context.context.device().clone(),
        context.context.submission().clone(),
        AllocationBudget(1 << 24),
    );
    let programs = programs
        .into_iter()
        .map(|(wgsl, declarations)| {
            common::graphics(
                &runtime,
                &wgsl,
                "vs",
                "fs",
                StageDeclarations::default(),
                declarations,
            )
        })
        .collect();
    Arc::new(Mutex::new(Parts {
        runtime,
        programs,
        draws: Vec::new(),
    }))
}

impl GameFeatureFactory for Keeper {
    fn name(&self) -> &'static str {
        "keeper"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![kept()]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let filter = wgpu::TextureSampleType::Float { filterable: true };
        let declarations = StageDeclarations {
            resources: ResourceLayout::new(vec![texture_binding(0, "keeper.kept", filter)]),
            constants: Vec::new(),
        };
        Ok(Box::new(Reader {
            targets: context.targets.clone(),
            seen: self.seen.clone(),
            parts: parts(context, vec![(READ_WGSL.to_owned(), declarations)]),
        }))
    }

    fn build_depth_prefix(
        &self,
        context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        Ok(Some(Box::new(Prefix {
            targets: context.targets.clone(),
            seen: self.seen.clone(),
            shadows: context.shadows.clone(),
            occlusion: context.occlusion.clone(),
            environment: context.environment.clone(),
            terrain: context.terrain.clone(),
            textures: context.scene_textures.clone(),
            parts: parts(
                context,
                vec![
                    (raise_wgsl(ORTHOGRAPHIC_BLOCK), StageDeclarations::default()),
                    (FILL_RED_WGSL.to_owned(), StageDeclarations::default()),
                ],
            ),
        })))
    }
}

fn empty() -> FeatureDeclaration {
    FeatureDeclaration {
        passes: Vec::new(),
        effects: Vec::new(),
    }
}

/// Draws one prepared draw per entry, as the frame's passes ask.
fn record(parts: &Mutex<Parts>, entry: PassEntry, pass: &mut wgpu::RenderPass<'_>) {
    let parts = parts.lock().unwrap();
    let (_, draw) = parts
        .draws
        .iter()
        .find(|(known, _)| *known == entry)
        .expect("prepared");
    draw.record(pass);
}

fn prepare_draw(
    parts: &mut Parts,
    context: &PrepareContext<'_>,
    (entry, program, count): (PassEntry, usize, u32),
    imports: &[Import],
) -> Result<(), FrameError> {
    let target = PassTarget::of(context, entry)?;
    let state = target.render_state(RasterState {
        cull: None,
        front_face: wgpu::FrontFace::Ccw,
        alpha_to_coverage: false,
    });
    let signature = target
        .signature()
        .map_err(|error| common::program_error("keeper", &error))?;
    let draw = parts
        .runtime
        .prepare_imported_draw(
            &ImportedDraw {
                program: &parts.programs[program],
                state: &state,
                vertex_inputs: &[],
                bindings: &[],
                imports,
                sets: &[],
                topology: wgpu::PrimitiveTopology::TriangleList,
                geometry: &ImportedGeometry::Vertices { first: 0, count },
                instances: 0..1,
                owners: &[],
            },
            signature,
        )
        .map_err(|error| common::program_error("keeper", &error))?;
    target.admit("keeper", &draw)?;
    parts.draws.push((entry, draw));
    Ok(())
}

#[derive(Clone)]
struct Reader {
    targets: GameTargets,
    seen: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for Reader {
    fn declare(&self) -> FeatureDeclaration {
        empty()
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        if !self.seen.lock().unwrap().read {
            return Ok(empty());
        }
        let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
            .writing([ColorOutput::Frame])
            .reading([kept().layer(0)]);
        Ok(FeatureDeclaration {
            passes: vec![self.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Reader {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        if !self.seen.lock().unwrap().read {
            return Ok(());
        }
        let mut parts = self.parts.lock().unwrap();
        if parts.draws.iter().any(|(entry, _)| *entry == READER) {
            return Ok(());
        }
        let texture = self.targets.texture(KEPT)?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let imports = [Import::texture(
            EngineSemantic::named("keeper.kept"),
            texture,
            view,
        )];
        prepare_draw(&mut parts, context, (READER, 0, 3), &imports)
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        record(&self.parts, context.entry, pass);
    }
}

#[derive(Clone)]
struct Prefix {
    targets: GameTargets,
    seen: Shared,
    shadows: FrameShadowHandle,
    occlusion: FrameOcclusionHandle,
    environment: FrameEnvironmentHandle,
    terrain: FrameTerrainHandle,
    textures: FrameSceneTexturesHandle,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for Prefix {
    fn declare(&self) -> FeatureDeclaration {
        empty()
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        if !passes
            .iter()
            .any(|pass| pass.slot == PassSlot::DepthNormalPrepass)
        {
            return Ok(empty());
        }
        let (raise, fill) = {
            let seen = self.seen.lock().unwrap();
            (seen.raise, seen.fill)
        };
        let mut declared = Vec::new();
        if raise {
            let spec = GamePassSpec::new(GamePlace::AfterDepthPrepass, 0)
                .writing([ColorOutput::Prepass(FrameProduct::normals().resource())])
                .with_depth(DepthAccess::ReadWrite);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        if fill {
            let spec = GamePassSpec::new(GamePlace::AfterDepthPrepass, 1)
                .writing([ColorOutput::Target(kept().layer(0))]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Prefix {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        // A frame where it planned no pass (no prepass, or neither knob set) still prepares it.
        if [RAISE, FILL]
            .iter()
            .all(|&entry| context.pass(entry).is_err())
        {
            return Ok(());
        }
        let (raise, fill) = {
            let seen = self.seen.lock().unwrap();
            (seen.raise, seen.fill)
        };
        let lent = format!(
            "shadow {:?}, occlusion {:?}, environment {:?}, terrain {:?}, textures {:?}",
            self.shadows.current().map(|_| true),
            self.occlusion
                .current()
                .map(|occlusion| occlusion.is_some()),
            self.environment
                .current()
                .map(|environment| environment.is_some()),
            self.terrain.current().map(|chunks| chunks.is_some()),
            self.textures.current().map(|textures| textures.len()),
        );
        self.seen.lock().unwrap().lent.push(lent);
        let mut parts = self.parts.lock().unwrap();
        parts.draws.clear();
        if raise {
            prepare_draw(&mut parts, context, (RAISE, 0, 6), &[])?;
        }
        if fill {
            prepare_draw(&mut parts, context, (FILL, 1, 3), &[])?;
        }
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        record(&self.parts, context.entry, pass);
    }
}

fn inventory(raise: bool, fill: bool, read: bool) -> (GamePasses, Shared) {
    let seen = Shared::default();
    *seen.lock().unwrap() = Seen {
        raise,
        fill,
        read,
        lent: Vec::new(),
    };
    let inventory = GamePasses::new().with(Arc::new(Keeper { seen: seen.clone() }));
    (inventory, seen)
}

fn knobs(seen: &Shared, raise: bool, fill: bool, read: bool) {
    let mut seen = seen.lock().unwrap();
    seen.raise = raise;
    seen.fill = fill;
    seen.read = read;
}

fn near(range: DepthRange) -> f32 {
    match range {
        DepthRange::Receivers { near, .. } => near,
        DepthRange::EmptyDepth => panic!("the quad is a receiver"),
    }
}

#[test]
fn a_shadowed_capture_admits_every_write_of_a_prefix_pass_over_several_frames() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (stock_inventory, _) = inventory(false, false, false);
    let (mut stock_rig, mut stock_scene) = shadowed(stock_inventory);
    let stock = capture_fit(&mut stock_rig, &mut stock_scene, 0);
    let (inventory, seen) = inventory(true, true, false);
    let (mut rig, mut scene) = shadowed(inventory);
    let first = capture_fit(&mut rig, &mut scene, 0);
    assert!(
        near(first.depth) < near(stock.depth) - 1.0,
        "the fit read the block's depth from the prefix: {:?} against {:?}",
        first.depth,
        stock.depth
    );
    assert!(
        rig.renderer.game_targets().is_written(KEPT, 0),
        "frame 0's prefix pass wrote the persistent target"
    );
    knobs(&seen, true, false, true);
    for frame in 1..4 {
        let fit = capture_fit(&mut rig, &mut scene, frame);
        assert_eq!(fit.depth, first.depth, "frame {frame} fits the same depth");
        assert_eq!(
            Rig::pixel(&rig.output(), 3, 20),
            [255, 0, 0, 255],
            "frame {frame} reads the target frame 0's prefix pass wrote"
        );
    }
}

#[test]
fn inside_a_separate_prefix_the_prefix_feature_reads_the_lent_terrain_and_textures_only() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (whole_game, whole_seen) = inventory(true, false, false);
    let source = quad_scene();
    let scene = source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(effects_with(Some(ssao()))),
        )
        .unwrap();
    let mut rig = Rig::with_ssao(&scene, SIZE, ssao(), |settings| {
        settings.game_passes = whole_game;
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    let whole = std::mem::take(&mut whole_seen.lock().unwrap().lent);
    let (captured_game, captured_seen) = inventory(true, false, false);
    let (mut rig, mut scene) = shadowed(captured_game);
    capture_fit(&mut rig, &mut scene, 0);
    let captured = std::mem::take(&mut captured_seen.lock().unwrap().lent);
    println!("whole frame: {whole:?}");
    println!("same-frame capture prefix: {captured:?}");
    assert_eq!(whole.len(), 1);
    assert_eq!(captured.len(), 1);
    assert_eq!(
        whole[0],
        "shadow Err(NotPrepared), occlusion Ok(true), environment Ok(false), terrain Ok(false), \
         textures Ok(1)",
        "a frame drawn whole lends its inputs to the depth-prefix feature (this unshadowed one \
         has no cascades, and its scene no terrain)"
    );
    assert_eq!(
        captured[0],
        "shadow Err(NotPrepared), occlusion Err(NotPrepared), environment Err(NotPrepared), \
         terrain Ok(false), textures Ok(1)",
        "inside a prefix the terrain (this scene has none) and the resident scene textures are \
         lent: the cascades are not fitted and SSAO has not run"
    );
}

/// The shadowed rig with screen-space ambient occlusion, whose same-frame capture suffix lists
/// the prepass entries the prefix completed.
fn shadowed_with_ssao(inventory: GamePasses) -> (Rig, ShadowedScene) {
    let source = quad_scene();
    let lit = source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(effects_with(Some(ssao()))),
        )
        .unwrap();
    let rig = Rig::with_ssao(&lit, SIZE, ssao(), |settings| {
        settings.game_passes = inventory;
    });
    let mut scene = ShadowedScene::new(
        &rig.context,
        PreparedScene::new(&lit).unwrap(),
        DirectionalShadowSettings {
            focus_meters: glam::Vec3::new(0.0, 0.0, 0.5),
            half_extent_meters: 4.0,
            depth_half_extent_meters: 4.0,
            resolution: 512,
            depth_bias: 0.001,
            normal_bias_meters: 0.01,
            refresh_policy: CascadeRefreshPolicy::default(),
        },
        ShadowedTarget::new(wgpu::TextureFormat::Rgba16Float, 1),
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
    (rig, scene)
}

#[test]
fn an_ssao_shadowed_capture_publishes_and_reads_what_a_prefix_pass_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (stock_inventory, _) = inventory(false, false, false);
    let (mut stock_rig, mut stock_scene) = shadowed_with_ssao(stock_inventory);
    let stock = capture_fit(&mut stock_rig, &mut stock_scene, 0);
    let (inventory, seen) = inventory(true, true, true);
    let (mut rig, mut scene) = shadowed_with_ssao(inventory);
    let first = capture_fit(&mut rig, &mut scene, 0);
    assert!(
        near(first.depth) < near(stock.depth) - 1.0,
        "the fit read the block's depth from the prefix: {:?} against {:?}",
        first.depth,
        stock.depth
    );
    assert!(
        rig.renderer.game_targets().is_written(KEPT, 0),
        "frame 0's prefix pass wrote the persistent target"
    );
    let red = |rig: &Rig| {
        let texel =
            rig.read_half(rig.renderer.targets().scene().texture())[20 * SIZE[0] as usize + 3];
        texel[0] > 0.9 && texel[1] < 0.05
    };
    assert!(red(&rig), "frame 0 read the target its prefix pass wrote");
    knobs(&seen, true, false, true);
    for frame in 1..4 {
        let fit = capture_fit(&mut rig, &mut scene, frame);
        assert_eq!(fit.depth, first.depth, "frame {frame} fits the same depth");
        assert!(red(&rig), "frame {frame} read the retained target");
    }
}
