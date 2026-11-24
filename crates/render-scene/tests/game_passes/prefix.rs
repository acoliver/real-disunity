//! A game's pass after the depth prepass draws depth and prepass normals the stock prepass does
//! not, so SSAO, depth fitting and Opaque see it: in a frame drawn whole, and inside the prefix a
//! same-frame fitted capture submits ahead of the frame.
use super::common;
use super::fixture::{Rig, SIZE, quad_scene};
use super::lighting::ssao;
use super::shadowed::{capture_fit, effects_with, shadowed};
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, ImportedDraw, ImportedGeometry, PreparedProgramDraw, ProgramRuntime,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    CameraPose, SceneView, Viewport,
    compositor::{
        ColorOutput, DepthAccess, FrameOcclusionHandle, GameBuild, GameFeatureFactory,
        GamePassSpec, GamePasses, GameTargets, PassTarget, prepass_writes_frame_depth,
    },
    frame::metadata::FrameProduct,
    shadows::sdsm::DepthRange,
    wgpu,
};

/// The central square of the screen at device depth `depth`, nearer the camera than the stock
/// quad behind it, with a view-space normal that faces the camera and covers the pixel.
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

/// Two metres above the stock quad for the straight-down orthographic camera, which puts the
/// ground at device depth 0.495.
const ORTHOGRAPHIC_BLOCK: f32 = 0.29;
/// How much nearer the camera the block is than the quad at the screen's centre, in metres.
const PERSPECTIVE_LIFT_METERS: f32 = 0.8;

const RAISE: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterDepthPrepass), 0);

/// A pixel of the stock quad just outside the block, and the block's centre.
const BESIDE: (usize, usize) = (22, 24);
const CENTRE: (usize, usize) = (32, 24);
/// Outside the stock quad: the frame's clear colour.
pub(super) const OUTSIDE: (usize, usize) = (3, 20);

/// What the game reports to its test.
#[derive(Default)]
struct Seen {
    /// Prefix preparations and draws, frame by frame.
    prepares: u32,
    draws: u32,
    /// Whether each planned frame's prepass wrote the depth Opaque draws with.
    shares_depth: Vec<bool>,
    /// The last frame's filtered occlusion.
    occlusion: Option<wgpu::Texture>,
}

type Shared = Arc<Mutex<Seen>>;

struct Raiser {
    /// The block's device depth; `None` adds no depth-prefix feature.
    raise: Option<f32>,
    seen: Shared,
}

impl GameFeatureFactory for Raiser {
    fn name(&self) -> &'static str {
        "raiser"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(Observer {
            occlusion: context.occlusion.clone(),
            seen: self.seen.clone(),
        }))
    }

    fn build_depth_prefix(
        &self,
        context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        let Some(depth) = self.raise else {
            return Ok(None);
        };
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let program = common::graphics(
            &runtime,
            &raise_wgsl(depth),
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations::default(),
        );
        Ok(Some(Box::new(Prefix {
            targets: context.targets.clone(),
            seen: self.seen.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                program,
                draw: None,
            })),
        })))
    }
}

/// The ordinary feature: draws nothing, and reports what the frame lends and plans.
#[derive(Clone)]
struct Observer {
    occlusion: FrameOcclusionHandle,
    seen: Shared,
}

impl FrameFeature for Observer {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        _views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        self.seen
            .lock()
            .unwrap()
            .shares_depth
            .push(prepass_writes_frame_depth(passes));
        Ok(self.declare())
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Observer {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.seen.lock().unwrap().occlusion =
            self.occlusion.current()?.map(|occlusion| occlusion.texture);
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

struct Parts {
    runtime: ProgramRuntime,
    program: GraphicsProgram,
    draw: Option<PreparedProgramDraw>,
}

/// The depth-prefix feature: the raised block, into the frame depth and the prepass normals.
#[derive(Clone)]
struct Prefix {
    targets: GameTargets,
    seen: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for Prefix {
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
        if !passes
            .iter()
            .any(|pass| pass.slot == PassSlot::DepthNormalPrepass)
        {
            return Ok(self.declare());
        }
        let spec = GamePassSpec::new(GamePlace::AfterDepthPrepass, 0)
            .writing([ColorOutput::Prepass(FrameProduct::normals().resource())])
            .with_depth(DepthAccess::ReadWrite);
        Ok(FeatureDeclaration {
            passes: vec![self.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Prefix {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        // A frame without the prepass plans no pass of this feature and still prepares it.
        if context.pass(RAISE).is_err() {
            return Ok(());
        }
        let target = PassTarget::of(context, RAISE)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("raiser", &error))?;
        let draw = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.program,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports: &[],
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
                    instances: 0..1,
                    owners: &[],
                },
                signature,
            )
            .map_err(|error| common::program_error("raiser", &error))?;
        target.admit("raiser", &draw)?;
        parts.draw = Some(draw);
        self.seen.lock().unwrap().prepares += 1;
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        self.parts
            .lock()
            .unwrap()
            .draw
            .as_ref()
            .expect("prepared")
            .record(pass);
        self.seen.lock().unwrap().draws += 1;
    }
}

fn inventory(raise: Option<f32>) -> (GamePasses, Shared) {
    let seen = Shared::default();
    let inventory = GamePasses::new().with(Arc::new(Raiser {
        raise,
        seen: seen.clone(),
    }));
    (inventory, seen)
}

/// A perspective camera five metres above the quad, tilted a little so the canonical up is not
/// the view direction.
pub(super) fn perspective() -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -1000, 5000],
            target_millimeters: [0, 0, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 20_000,
        },
        Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap()
}

/// The device depth of the point `PERSPECTIVE_LIFT_METERS` towards the eye from the quad's centre.
fn lifted_depth(camera: &SceneView) -> f32 {
    let eye = glam::Vec3::new(0.0, -1.0, 5.0);
    let point = eye.normalize() * PERSPECTIVE_LIFT_METERS;
    let clip = camera.view_projection_matrix() * point.extend(1.0);
    clip.z / clip.w
}

/// One HDR frame with SSAO seen from [`perspective`], and what it drew: the frame colour and the
/// filtered visibility.
fn ssao_frame(raise: bool) -> (Vec<[f32; 4]>, Vec<f32>, Shared) {
    let camera = perspective();
    let (inventory, seen) = inventory(raise.then(|| lifted_depth(&camera)));
    let source = quad_scene();
    let scene = source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(effects_with(Some(ssao()))),
        )
        .unwrap();
    let mut rig = Rig::with_ssao_view(&scene, (SIZE, camera), ssao(), |settings| {
        settings.game_passes = inventory;
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    let colour = rig.read_half(rig.renderer.targets().texture());
    let occlusion = seen.lock().unwrap().occlusion.clone().expect("SSAO ran");
    assert_eq!(occlusion.format(), wgpu::TextureFormat::R32Float);
    assert_eq!([occlusion.width(), occlusion.height()], SIZE);
    let visibility = rig
        .read(&occlusion)
        .into_iter()
        .map(f32::from_le_bytes)
        .collect();
    (colour, visibility, seen)
}

pub(super) fn at<T: Copy>(image: &[T], (column, row): (usize, usize)) -> T {
    image[row * SIZE[0] as usize + column]
}

#[test]
fn ssao_and_opaque_see_what_a_game_pass_after_the_prepass_drew() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (stock_colour, stock, stock_seen) = ssao_frame(false);
    let (colour, raised, seen) = ssao_frame(true);
    assert_eq!(stock_seen.lock().unwrap().shares_depth, [true]);
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.shares_depth,
        [true],
        "the prepass writes Opaque's depth"
    );
    assert_eq!((seen.prepares, seen.draws), (1, 1));
    assert!(
        at(&stock, BESIDE) > 0.9,
        "the flat stock quad leaves the ground unoccluded: {}",
        at(&stock, BESIDE)
    );
    assert!(
        at(&raised, BESIDE) < at(&stock, BESIDE) - 0.05,
        "SSAO saw the block beside the ground pixel: {} against {}",
        at(&raised, BESIDE),
        at(&stock, BESIDE)
    );
    assert!(at(&raised, CENTRE) > 0.9, "the block's flat top is open");
    let bits = |pixel: [f32; 4]| pixel.map(f32::to_bits);
    assert_eq!(
        bits(at(&stock_colour, CENTRE)),
        bits(at(&stock_colour, BESIDE)),
        "without the block, Opaque draws the stock quad at the centre"
    );
    assert_eq!(
        bits(at(&colour, CENTRE)),
        bits(at(&colour, OUTSIDE)),
        "the block's depth completed Opaque's, so the stock quad behind it is not drawn"
    );
    assert_eq!(bits(at(&colour, BESIDE)), bits(at(&stock_colour, BESIDE)));
}

fn near(range: DepthRange) -> f32 {
    match range {
        DepthRange::Receivers { near, .. } => near,
        DepthRange::EmptyDepth => panic!("the quad is a receiver"),
    }
}

#[test]
fn a_same_frame_fitted_capture_runs_the_games_prefix_pass_once_inside_the_prefix() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (stock_inventory, _) = inventory(None);
    let (mut stock_rig, mut stock_scene) = shadowed(stock_inventory);
    let stock = capture_fit(&mut stock_rig, &mut stock_scene, 0);
    let (inventory, seen) = inventory(Some(ORTHOGRAPHIC_BLOCK));
    let (mut rig, mut scene) = shadowed(inventory);
    let first = capture_fit(&mut rig, &mut scene, 0);
    let second = capture_fit(&mut rig, &mut scene, 1);
    assert!(
        near(first.depth) < near(stock.depth) - 1.0,
        "the fit read the block's depth from the prefix: {:?} against {:?}",
        first.depth,
        stock.depth
    );
    assert_eq!(first.depth, second.depth);
    let seen = seen.lock().unwrap();
    assert_eq!(
        (seen.prepares, seen.draws),
        (2, 2),
        "prepared and drawn once per frame, in the prefix only"
    );
}
