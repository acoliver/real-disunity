//! A game's passes at `GamePlace::BeforePost` in an HDR frame: a blended quad over the lit scene
//! colour, a soft fade that tests the frame's depth read-only and samples it in the same pass,
//! and a frame whose game plans nothing there.
use super::common::{self, texture_binding};
use super::fixture::{Rig, SIZE, quad_scene};
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
use worldrunner_render_program::state::{ColorState, RasterState, RenderState};
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, DepthAccess, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses,
        GameTargets, PassTarget,
    },
    wgpu,
};

/// Six vertices of a clip-space rectangle `[x0, y0, x1, y1]` at depth `z`.
fn rectangle(name: &str, rect: [f32; 4], z: f32) -> String {
    let [x0, y0, x1, y1] = rect;
    format!(
        "fn {name}(k: u32) -> vec4<f32> {{
    var corners = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    let c = corners[k];
    return vec4<f32>(mix({x0:?}, {x1:?}, c.x), mix({y0:?}, {y1:?}, c.y), {z:?}, 1.0);
}}"
    )
}

/// The left half of the frame, where [`Mode::Blend`] fills magenta after the transparent passes.
const FILL_RECT: [f32; 4] = [-1.0, -1.0, 0.0, 1.0];
/// The middle of the frame, where the blended quad lands: over the magenta on its left half and
/// over the stock frame on its right half.
const BLEND_RECT: [f32; 4] = [-0.5, -0.5, 0.5, 0.5];
/// The colour and coverage of the blended quad.
const SOURCE: [f32; 3] = [0.2, 0.4, 0.8];
const ALPHA: f32 = 0.25;
/// The depths of the soft quads: in front of the stock quad on the left of the frame, behind it
/// on the right.
const NEAR: f32 = 0.3;
const FAR: f32 = 0.7;
/// The depth distance over which the soft quads fade in.
const FADE_RANGE: f32 = 0.5;

fn fill_shader() -> String {
    format!(
        "{}
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{ return fill(i); }}
@fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>(1.0, 0.0, 1.0, 1.0); }}",
        rectangle("fill", FILL_RECT, 0.5)
    )
}

fn blend_shader(alpha: f32) -> String {
    let [r, g, b] = SOURCE;
    format!(
        "{}
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{ return quad(i); }}
@fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>({r:?}, {g:?}, {b:?}, {alpha:?}); }}",
        rectangle("quad", BLEND_RECT, 0.5)
    )
}

/// Two quads covering the frame, the left one at [`NEAR`] and the right one at [`FAR`]. Each
/// fragment that passes the depth test writes the fade `(scene depth - its depth) / range`,
/// clamped, in red, the sampled scene depth in green and its own depth in blue.
fn soft_shader() -> String {
    format!(
        "{}
{}
@group(0) @binding(0) var scene_depth: texture_depth_2d;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    if i < 6u {{ return left(i); }}
    return right(i - 6u);
}}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
    let scene = textureLoad(scene_depth, vec2<i32>(p.xy), 0);
    return vec4<f32>(clamp((scene - p.z) / {FADE_RANGE:?}, 0.0, 1.0), scene, p.z, 1.0);
}}",
        rectangle("left", [-1.0, -1.0, 0.0, 1.0], NEAR),
        rectangle("right", [0.0, -1.0, 1.0, 1.0], FAR),
    )
}

/// What the game plans each frame.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// No pass at all.
    Nothing,
    /// A magenta fill after the transparent passes, then the blended quad before post.
    Blend,
    /// Only the blended quad before post, with no coverage.
    Invisible,
    /// The soft quads before post.
    Soft,
}

const FILL: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterTransparent), 0);
const BEFORE_POST: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforePost), 0);

const ALPHA_OVER_RGB: ColorState = ColorState {
    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
    write_mask: wgpu::ColorWrites::COLOR,
};

struct ParticleGame {
    mode: Mode,
    recorded: Arc<Mutex<Vec<PassEntry>>>,
}

impl GameFeatureFactory for ParticleGame {
    fn name(&self) -> &'static str {
        "before-post game"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let program = |wgsl: &str, bindings| {
            common::graphics(
                &runtime,
                wgsl,
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
            fill: program(&fill_shader(), Vec::new()),
            blend: program(
                &blend_shader(if self.mode == Mode::Invisible {
                    0.0
                } else {
                    ALPHA
                }),
                Vec::new(),
            ),
            soft: program(
                &soft_shader(),
                vec![texture_binding(
                    0,
                    "scene.depth",
                    wgpu::TextureSampleType::Depth,
                )],
            ),
        };
        Ok(Box::new(ParticleFeature {
            mode: self.mode,
            targets: context.targets.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                programs,
                draws: Vec::new(),
            })),
            recorded: self.recorded.clone(),
        }))
    }
}

struct Programs {
    fill: GraphicsProgram,
    blend: GraphicsProgram,
    soft: GraphicsProgram,
}

struct Parts {
    runtime: ProgramRuntime,
    programs: Programs,
    draws: Vec<(PassEntry, PreparedProgramDraw)>,
}

struct ParticleFeature {
    mode: Mode,
    targets: GameTargets,
    parts: Arc<Mutex<Parts>>,
    recorded: Arc<Mutex<Vec<PassEntry>>>,
}

impl ParticleFeature {
    fn specs(&self) -> Vec<GamePassSpec> {
        let before_post = GamePassSpec::new(GamePlace::BeforePost, 0).writing([ColorOutput::Frame]);
        match self.mode {
            Mode::Nothing => Vec::new(),
            Mode::Blend => vec![
                GamePassSpec::new(GamePlace::AfterTransparent, 0).writing([ColorOutput::Frame]),
                before_post,
            ],
            Mode::Invisible => vec![before_post],
            Mode::Soft => vec![before_post.with_depth(DepthAccess::ReadOnlySampled)],
        }
    }
}

impl FrameFeature for ParticleFeature {
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
        let mut declared = Vec::new();
        for spec in self.specs() {
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(ParticleFrame {
            mode: self.mode,
            parts: self.parts.clone(),
            recorded: self.recorded.clone(),
        }))
    }
}

struct ParticleFrame {
    mode: Mode,
    parts: Arc<Mutex<Parts>>,
    recorded: Arc<Mutex<Vec<PassEntry>>>,
}

const RASTER: RasterState = RasterState {
    cull: None,
    front_face: wgpu::FrontFace::Ccw,
    alpha_to_coverage: false,
};

impl ParticleFrame {
    /// The program, state, imports and vertex count of the draw into `entry`.
    fn draw_of<'p>(
        &self,
        context: &PrepareContext<'_>,
        programs: &'p Programs,
        entry: PassEntry,
    ) -> Result<(&'p GraphicsProgram, RenderState, Vec<Import>, u32), FrameError> {
        let target = PassTarget::of(context, entry)?;
        if entry == FILL {
            return Ok((&programs.fill, target.render_state(RASTER), Vec::new(), 6));
        }
        if self.mode != Mode::Soft {
            let mut state = target.render_state(RASTER);
            state.colors[0] = Some(ALPHA_OVER_RGB);
            return Ok((&programs.blend, state, Vec::new(), 6));
        }
        let depth = PassTarget::sampled_depth(context, entry)?;
        let imports = vec![Import::texture(
            EngineSemantic::named("scene.depth"),
            depth.texture().clone(),
            depth.sampled_view(),
        )];
        Ok((&programs.soft, target.render_state(RASTER), imports, 12))
    }
}

impl PreparedFeature for ParticleFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        parts.draws.clear();
        let entries: &[PassEntry] = match self.mode {
            Mode::Nothing => &[],
            Mode::Blend => &[FILL, BEFORE_POST],
            Mode::Invisible | Mode::Soft => &[BEFORE_POST],
        };
        for entry in entries {
            let (program, state, imports, count) =
                self.draw_of(context, &parts.programs, *entry)?;
            let target = PassTarget::of(context, *entry)?;
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
                        geometry: &ImportedGeometry::Vertices { first: 0, count },
                        instances: 0..1,
                        owners: &[],
                    },
                    target.signature().unwrap(),
                )
                .map_err(|error| common::program_error("before-post game", &error))?;
            target.admit("before-post game", &draw)?;
            parts.draws.push((*entry, draw));
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
        self.recorded.lock().unwrap().push(context.entry);
    }
}

/// Renders one HDR frame of the stock quad with the game in `mode` (or none) and returns the
/// frame's scene colour as the post chain would read it, with the passes the game recorded.
fn render(mode: Option<Mode>) -> (Vec<[f32; 4]>, Vec<PassEntry>) {
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let game = mode.map(|mode| {
        Arc::new(ParticleGame {
            mode,
            recorded: recorded.clone(),
        })
    });
    let mut rig = Rig::hdr(&quad_scene(), SIZE, |settings| {
        if let Some(game) = game {
            settings.game_passes = GamePasses::new().with(game);
        }
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    let image = rig.read_half(rig.renderer.targets().scene().texture());
    let recorded = recorded.lock().unwrap().clone();
    (image, recorded)
}

fn at(image: &[[f32; 4]], column: usize, row: usize) -> [f32; 4] {
    image[row * SIZE[0] as usize + column]
}

/// Half floats hold about three decimal digits; the blend and the fade are computed in at least
/// single precision and stored once.
fn assert_near(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 2e-3,
        "{what}: {actual} against {expected}"
    );
}

#[test]
fn a_pass_before_post_alpha_blends_over_the_lit_colour_the_transparent_passes_left() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (stock, _) = render(None);
    let (image, recorded) = render(Some(Mode::Blend));
    assert_eq!(recorded, [FILL, BEFORE_POST]);
    let row = 24;
    // The blended quad spans columns 16 to 47: 20 and 28 lie over the magenta fill of the left
    // half, 36 and 44 over the stock quad (columns 14 to 49). Column 4 is the fill alone.
    for (column, under) in [
        (20, [1.0, 0.0, 1.0]),
        (28, [1.0, 0.0, 1.0]),
        (36, at(&stock, 36, row)[..3].try_into().unwrap()),
        (44, at(&stock, 44, row)[..3].try_into().unwrap()),
    ] {
        let pixel = at(&image, column, row);
        for channel in 0..3 {
            assert_near(
                pixel[channel],
                ALPHA * SOURCE[channel] + (1.0 - ALPHA) * under[channel],
                &format!("column {column} channel {channel}"),
            );
        }
        let below = if column < 32 {
            1.0
        } else {
            at(&stock, column, row)[3]
        };
        assert_eq!(
            pixel[3].to_bits(),
            below.to_bits(),
            "column {column}: the RGB write mask keeps the alpha"
        );
    }
    assert_eq!(
        at(&image, 4, row).map(f32::to_bits),
        [1.0_f32, 0.0, 1.0, 1.0].map(f32::to_bits),
        "the fill alone"
    );
    for column in 48..64 {
        assert_eq!(
            at(&image, column, row).map(f32::to_bits),
            at(&stock, column, row).map(f32::to_bits),
            "column {column}: outside the quad and the fill"
        );
    }
}

#[test]
fn a_soft_pass_tests_the_frame_depth_read_only_and_samples_it_in_the_same_pass() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (stock, _) = render(None);
    let (image, recorded) = render(Some(Mode::Soft));
    assert_eq!(recorded, [BEFORE_POST]);
    // The fixture's straight-down orthographic view puts the stock quad at this device depth;
    // the cleared depth around it is 1.
    let quad = (5.0 - 0.1) / (10.0 - 0.1);
    let fade = |scene: f32, z: f32| ((scene - z) / FADE_RANGE).clamp(0.0, 1.0);
    let row = 24;
    for (column, scene, z) in [(20, quad, NEAR), (4, 1.0, NEAR), (58, 1.0, FAR)] {
        let pixel = at(&image, column, row);
        assert_near(pixel[0], fade(scene, z), &format!("column {column} fade"));
        assert_near(pixel[1], scene, &format!("column {column} sampled depth"));
        assert_near(pixel[2], z, &format!("column {column} fragment depth"));
        assert_eq!(
            pixel[3].to_bits(),
            1.0_f32.to_bits(),
            "column {column} alpha"
        );
    }
    // The right quad lies behind the stock quad, so the read-only depth test rejects it there and
    // the stock colour stays.
    for column in [36, 44] {
        assert_eq!(
            at(&image, column, row).map(f32::to_bits),
            at(&stock, column, row).map(f32::to_bits),
            "column {column}: behind the stock quad"
        );
    }
}

#[test]
fn the_stock_frame_is_unchanged_by_a_game_with_nothing_or_nothing_visible_before_post() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let bits = |image: Vec<[f32; 4]>| -> Vec<[u32; 4]> {
        image.iter().map(|pixel| pixel.map(f32::to_bits)).collect()
    };
    let (stock, _) = render(None);
    let stock = bits(stock);
    let (idle, recorded) = render(Some(Mode::Nothing));
    assert!(recorded.is_empty());
    assert_eq!(bits(idle), stock, "a game that plans no pass");
    let (invisible, recorded) = render(Some(Mode::Invisible));
    assert_eq!(recorded, [BEFORE_POST]);
    assert_eq!(
        bits(invisible),
        stock,
        "a pass before post that blends with no coverage"
    );
}
