//! A target that several of a game's passes write, some inside the depth prefix and some in the
//! rest of the frame: each later writer keeps what the earlier ones wrote, whether the prefix is
//! encoded in the frame or submitted ahead of it, with and without screen-space metadata.
use super::common;
use super::fixture::{Rig, SIZE, quad_scene};
use super::lighting::ssao;
use super::prefix::{OUTSIDE, at, perspective};
use super::shadowed::{capture, capture_fit, effects_with, fitted, shadowed};
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
        ColorOutput, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses, GameTargets,
        PassTarget, TargetDeclaration, TargetExtent,
    },
    shadows::ShadowedScene,
    wgpu,
};

/// A target that two passes of the depth prefix and one pass of the rest of the frame each write
/// a band of, read by a later pass of the frame.
const STACK: TargetDeclaration = TargetDeclaration::new(
    "stacker stack",
    wgpu::TextureFormat::Rgba8Unorm,
    TargetExtent::Fixed([4, 4]),
);
const STACK_SEMANTIC: &str = "stacker.stack";

/// Writes the columns of the stack between clip-space `x0` and `x1` with `colour`.
fn band_wgsl(x0: f32, x1: f32, colour: [f32; 3]) -> String {
    let [red, green, blue] = colour;
    format!(
        "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    var corners = array<vec2<f32>, 6>(
        vec2<f32>({x0:?}, -1.0), vec2<f32>({x1:?}, -1.0), vec2<f32>({x1:?}, 1.0),
        vec2<f32>({x0:?}, -1.0), vec2<f32>({x1:?}, 1.0), vec2<f32>({x0:?}, 1.0));
    return vec4<f32>(corners[i], 0.0, 1.0);
}}
@fragment fn fs() -> @location(0) vec4<f32> {{
    return vec4<f32>({red:?}, {green:?}, {blue:?}, 1.0);
}}"
    )
}

/// Writes the frame's colour from the red of stack column 2, which only the first prefix pass
/// writes, the green of column 0, which the second prefix pass writes, and the blue of column 3,
/// which the frame's own pass writes.
const STACK_READ_WGSL: &str = "
@group(0) @binding(0) var stack: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let red = textureLoad(stack, vec2<i32>(2, 0), 0).r;
    let green = textureLoad(stack, vec2<i32>(0, 0), 0).g;
    let blue = textureLoad(stack, vec2<i32>(3, 0), 0).b;
    return vec4<f32>(red, green, blue, 1.0);
}";

/// One pass of a stacker feature: what it plans and the shader it draws.
#[derive(Clone)]
struct Band {
    spec: GamePassSpec,
    wgsl: String,
}

impl Band {
    fn filling(place: GamePlace, occurrence: u32, x: [f32; 2], colour: [f32; 3]) -> Self {
        Self {
            spec: GamePassSpec::new(place, occurrence)
                .writing([ColorOutput::Target(STACK.layer(0))]),
            wgsl: band_wgsl(x[0], x[1], colour),
        }
    }

    fn reading() -> Self {
        Self {
            spec: GamePassSpec::new(GamePlace::BeforeOpaque, 10)
                .writing([ColorOutput::Frame])
                .reading([STACK.layer(0)]),
            wgsl: STACK_READ_WGSL.to_owned(),
        }
    }
}

/// The prefix's passes: the whole stack red, then, with `second`, its left half green.
fn prefix_bands(second: bool) -> Vec<Band> {
    let place = GamePlace::AfterDepthPrepass;
    let mut bands = vec![Band::filling(place, 0, [-1.0, 1.0], [1.0, 0.0, 0.0])];
    if second {
        bands.push(Band::filling(place, 1, [-1.0, 0.0], [0.0, 1.0, 0.0]));
    }
    bands
}

/// The frame's passes: the stack's last column blue, then the frame colour from the stack.
fn frame_bands() -> Vec<Band> {
    vec![
        Band::filling(GamePlace::BeforeOpaque, 0, [0.5, 1.0], [0.0, 0.0, 1.0]),
        Band::reading(),
    ]
}

struct Stacker {
    /// Whether the prefix writes the stack twice.
    second: bool,
}

impl Stacker {
    fn feature(context: &GameBuild<'_>, bands: Vec<Band>) -> Box<dyn FrameFeature> {
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let sampled = wgpu::TextureSampleType::Float { filterable: true };
        let programs = bands
            .iter()
            .map(|band| {
                let fragment = if band.spec.reads.is_empty() {
                    StageDeclarations::default()
                } else {
                    StageDeclarations {
                        resources: ResourceLayout::new(vec![common::texture_binding(
                            0,
                            STACK_SEMANTIC,
                            sampled,
                        )]),
                        constants: Vec::new(),
                    }
                };
                common::graphics(
                    &runtime,
                    &band.wgsl,
                    "vs",
                    "fs",
                    StageDeclarations::default(),
                    fragment,
                )
            })
            .collect();
        Box::new(Stack {
            targets: context.targets.clone(),
            bands,
            parts: Arc::new(Mutex::new(StackParts {
                runtime,
                programs,
                draws: Vec::new(),
            })),
        })
    }
}

impl GameFeatureFactory for Stacker {
    fn name(&self) -> &'static str {
        "stacker"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![STACK]
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Self::feature(context, frame_bands()))
    }

    fn build_depth_prefix(
        &self,
        context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        Ok(Some(Self::feature(context, prefix_bands(self.second))))
    }
}

struct StackParts {
    runtime: ProgramRuntime,
    /// One program for each band, in the same order.
    programs: Vec<GraphicsProgram>,
    draws: Vec<(PassEntry, PreparedProgramDraw)>,
}

/// A stacker feature: the prefix's bands or the frame's.
#[derive(Clone)]
struct Stack {
    targets: GameTargets,
    bands: Vec<Band>,
    parts: Arc<Mutex<StackParts>>,
}

impl FrameFeature for Stack {
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
        if self.targets.depth_prefix()
            && !passes
                .iter()
                .any(|pass| pass.slot == PassSlot::DepthNormalPrepass)
        {
            return Ok(self.declare());
        }
        let passes = self
            .bands
            .iter()
            .map(|band| self.targets.plan_pass(&band.spec, passes, views))
            .collect::<Result<_, _>>()?;
        Ok(FeatureDeclaration {
            passes,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl Stack {
    fn prepare_band(&self, context: &PrepareContext<'_>, index: usize) -> Result<(), FrameError> {
        let band = &self.bands[index];
        let entry = band.spec.entry();
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        if parts.draws.iter().any(|(known, _)| *known == entry) {
            return Ok(());
        }
        let target = PassTarget::of(context, entry)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        let imports = if band.spec.reads.is_empty() {
            Vec::new()
        } else {
            let texture = self.targets.texture(STACK.name)?;
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            vec![Import::texture(
                EngineSemantic::named(STACK_SEMANTIC),
                texture,
                view,
            )]
        };
        let draw = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.programs[index],
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports: &imports,
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 6 },
                    instances: 0..1,
                    owners: &[],
                },
                target.signature().unwrap(),
            )
            .map_err(|error| common::program_error("stacker", &error))?;
        target.admit("stacker", &draw)?;
        parts.draws.push((entry, draw));
        Ok(())
    }
}

impl PreparedFeature for Stack {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        // The prefix feature in a frame without the prepass plans no band and still prepares.
        (0..self.bands.len())
            .filter(|&index| context.pass(self.bands[index].spec.entry()).is_ok())
            .try_for_each(|index| self.prepare_band(context, index))
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

fn stacker(second: bool) -> GamePasses {
    GamePasses::new().with(Arc::new(Stacker { second }))
}

/// The stock quad under the shadowed scene's sun, with SSAO, so the renderer plans the depth
/// prepass and the screen-space passes in every frame.
fn screen_scene() -> worldrunner_render_plan::RenderScene {
    let source = quad_scene();
    source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(effects_with(Some(ssao()))),
        )
        .unwrap()
}

/// The frame colour beside the stock quad, after a fitted capture of `frame` by an HDR rig.
fn screen_capture(rig: &mut Rig, scene: &mut ShadowedScene, frame: u64) -> [f32; 4] {
    capture_fit(rig, scene, frame);
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.renderer.encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    at(&rig.read_half(rig.renderer.targets().texture()), OUTSIDE)
}

/// Asserts that red, green and blue of an HDR frame pixel each show the stack channel the frame
/// read: well above zero, where a cleared stack would leave zero.
fn assert_stacked(colour: [f32; 4], context: &str) {
    assert!(
        colour[..3].iter().all(|channel| *channel > 0.25),
        "{context}: red from the first prefix pass, green from the second, blue from the frame's \
         own pass: {colour:?}"
    );
}

#[test]
fn a_fitted_capture_keeps_what_each_prefix_pass_and_the_frame_wrote_to_one_target() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, mut scene) = shadowed(stacker(true));
    for frame in 0..2 {
        let image = capture(&mut rig, &mut scene, frame);
        assert_eq!(
            Rig::pixel(&image, OUTSIDE.0, OUTSIDE.1),
            [255, 255, 255, 255],
            "frame {frame}: red from the first prefix pass, green from the second, blue from the \
             frame's own pass"
        );
    }
}

#[test]
fn a_frame_pass_loads_a_target_the_separately_submitted_prefix_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, mut scene) = shadowed(stacker(false));
    for frame in 0..2 {
        let image = capture(&mut rig, &mut scene, frame);
        assert_eq!(
            Rig::pixel(&image, OUTSIDE.0, OUTSIDE.1),
            [255, 0, 255, 255],
            "frame {frame}: red from the prefix, blue from the frame's own pass"
        );
    }
}

#[test]
fn a_fitted_capture_with_screen_effects_keeps_what_the_prefix_and_the_frame_wrote() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let source = screen_scene();
    let mut rig = Rig::with_ssao(&source, SIZE, ssao(), |settings| {
        settings.game_passes = stacker(true);
    });
    let mut scene = fitted(&rig, &source, wgpu::TextureFormat::Rgba16Float);
    for frame in 0..2 {
        assert_stacked(
            screen_capture(&mut rig, &mut scene, frame),
            &format!("frame {frame}"),
        );
    }
}

#[test]
fn a_frame_drawn_whole_keeps_what_each_prefix_pass_wrote_to_one_target() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut rig = Rig::with_ssao_view(&screen_scene(), (SIZE, perspective()), ssao(), |settings| {
        settings.game_passes = stacker(true);
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    assert_stacked(
        at(&rig.read_half(rig.renderer.targets().texture()), OUTSIDE),
        "frame drawn whole",
    );
}
