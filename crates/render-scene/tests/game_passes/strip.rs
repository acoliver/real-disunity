//! A game that draws a strip into a four-colour set before Opaque and lights it with a fullscreen
//! pass that samples the set and writes the frame's colour. Both passes run on compiled program
//! draws prepared against the target the renderer reports for their entry.
use super::common::{self, Counts, graphics, texture_binding};
use std::sync::{Arc, Mutex};
use wgpu::TextureFormat;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::AttachmentView;
use worldrunner_render_post::BuildContext;
use worldrunner_render_post::{
    DrawContext, FeatureDeclaration, FrameError, FrameFeature, GamePlace, PassDeclaration,
    PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::binding::ResourceLayout;
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedVertexInput,
    PreparedProgramDraw, ProgramRuntime, TargetSignature,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, DepthAccess, FrameCameraHandle, GameBuild, GameFeatureFactory, GamePassSpec,
        GameTargets, PassTarget, StockReplacement, TargetDeclaration, TargetExtent,
    },
    wgpu,
};

pub const STRIP_WGSL: &str = "
@vertex fn vs(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
struct Out {
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
    @location(2) c: vec4<f32>,
    @location(3) d: u32,
}
@fragment fn fs() -> Out {
    return Out(
        vec4<f32>(1.0, 0.0, 0.0, 1.0),
        vec4<f32>(0.0, 1.0, 0.0, 1.0),
        vec4<f32>(0.0, 0.0, 0.50196078, 1.0),
        7u,
    );
}";

pub const LIGHT_WGSL: &str = "
@group(0) @binding(0) var g0: texture_2d<f32>;
@group(0) @binding(1) var g1: texture_2d<f32>;
@group(0) @binding(2) var g2: texture_2d<f32>;
@group(0) @binding(3) var g3: texture_2d<u32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let t = vec2<i32>(p.xy);
    let a = textureLoad(g0, t, 0);
    let b = textureLoad(g1, t, 0);
    let c = textureLoad(g2, t, 0);
    let d = textureLoad(g3, t, 0).x;
    if (c.a == 0.0) {
        discard;
    }
    return vec4<f32>(a.r, b.g, c.b, select(0.0, 1.0, d == 7u));
}";

/// The four formats of a game's G-buffer, in attachment order.
pub const FORMATS: [TextureFormat; 4] = [
    TextureFormat::Rgb10a2Unorm,
    TextureFormat::Rgba8UnormSrgb,
    TextureFormat::Rgba8Unorm,
    TextureFormat::R16Uint,
];
pub const NAMES: [&str; 4] = ["strip0", "strip1", "strip2", "strip3"];
pub const STRIP: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
pub const LIGHT: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 1);
/// The light pass when the game places it after Opaque instead.
pub const LIGHT_AFTER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::AfterOpaque), 0);

/// What the test chooses and what the feature reports back.
#[derive(Default)]
pub struct Knobs {
    /// The strip's clip-space rectangle `[x0, y0, x1, y1]` and its depth.
    pub rect: [f32; 4],
    pub depth: f32,
    /// Prepare the strip's draw for a pass with one colour attachment instead of its own.
    pub wrong_signature: bool,
    /// Leave the strip pass out of the plan, so the light pass reads targets nothing writes.
    pub skip_strip: bool,
    /// Plan the strip pass without the frame's depth.
    pub without_depth: bool,
    /// Place the light pass after Opaque, where it draws over what the stock object drew.
    pub light_after_opaque: bool,
    /// The stock surfaces the game draws, declared when the factory is installed.
    pub replaces: StockReplacement,
    pub counts: Counts,
    /// The program runtime's counters after the latest preparation.
    pub stats: Option<worldrunner_render_program::runtime::RuntimeStats>,
}

pub type Shared = Arc<Mutex<Knobs>>;

pub fn knobs(rect: [f32; 4], depth: f32) -> Shared {
    Arc::new(Mutex::new(Knobs {
        rect,
        depth,
        ..Knobs::default()
    }))
}

pub struct StripGame {
    pub knobs: Shared,
}

impl GameFeatureFactory for StripGame {
    fn name(&self) -> &'static str {
        "strip game"
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.knobs.lock().unwrap().replaces
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        NAMES
            .into_iter()
            .zip(FORMATS)
            .map(|(name, format)| TargetDeclaration::new(name, format, TargetExtent::Frame))
            .collect()
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        self.knobs.lock().unwrap().counts.builds += 1;
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let strip = graphics(
            &runtime,
            STRIP_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations::default(),
        );
        let light = graphics(
            &runtime,
            LIGHT_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            light_declarations(),
        );
        Ok(Box::new(Strip {
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            knobs: self.knobs.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                strip,
                light,
                draws: None,
            })),
        }))
    }
}

fn light_entry(after_opaque: bool) -> PassEntry {
    if after_opaque { LIGHT_AFTER } else { LIGHT }
}

fn light_declarations() -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            texture_binding(
                0,
                "strip.g0",
                wgpu::TextureSampleType::Float { filterable: true },
            ),
            texture_binding(
                1,
                "strip.g1",
                wgpu::TextureSampleType::Float { filterable: true },
            ),
            texture_binding(
                2,
                "strip.g2",
                wgpu::TextureSampleType::Float { filterable: true },
            ),
            texture_binding(3, "strip.g3", wgpu::TextureSampleType::Uint),
        ]),
        constants: Vec::new(),
    }
}

struct Parts {
    runtime: ProgramRuntime,
    strip: GraphicsProgram,
    light: GraphicsProgram,
    draws: Option<Draws>,
}

struct Draws {
    strip: Option<PreparedProgramDraw>,
    light: PreparedProgramDraw,
}

struct Strip {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl FrameFeature for Strip {
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
        let set: Vec<ResourceRef> = NAMES.map(ResourceRef::new).to_vec();
        let mut declared = Vec::new();
        if !knobs.skip_strip {
            let mut spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
                .writing(set.iter().copied().map(ColorOutput::Target));
            if !knobs.without_depth {
                spec = spec.with_depth(DepthAccess::ReadWrite);
            }
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        let (place, occurrence) = if knobs.light_after_opaque {
            (GamePlace::AfterOpaque, 0)
        } else {
            (GamePlace::BeforeOpaque, 1)
        };
        let light = GamePassSpec::new(place, occurrence)
            .writing([ColorOutput::Frame])
            .reading(set);
        declared.push(self.targets.plan_pass(&light, passes, views)?);
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(StripFrame {
            targets: self.targets.clone(),
            camera: self.camera.clone(),
            knobs: self.knobs.clone(),
            parts: self.parts.clone(),
        }))
    }
}

struct StripFrame {
    targets: GameTargets,
    camera: FrameCameraHandle,
    knobs: Shared,
    parts: Arc<Mutex<Parts>>,
}

impl StripFrame {
    fn raster(&self) -> Result<RasterState, FrameError> {
        Ok(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        })
    }

    fn prepare_draws(&self, context: &PrepareContext<'_>) -> Result<Draws, FrameError> {
        let knobs = self.knobs.lock().unwrap();
        let mut guard = self.parts.lock().unwrap();
        let parts = &mut *guard;
        let raster = self.raster()?;
        let light_target = PassTarget::of(context, light_entry(knobs.light_after_opaque))?;
        let strip = if knobs.skip_strip {
            None
        } else {
            let strip_target = PassTarget::of(context, STRIP)?;
            let signature = if knobs.wrong_signature {
                // Everything but one colour format matches the pass, so the runtime accepts the
                // draw and only the pass's own check can refuse it.
                let mut colors = strip_target.color_formats().to_vec();
                colors[1] = TextureFormat::Rgba8Unorm;
                TargetSignature::attachments(
                    &colors,
                    strip_target.depth_format(),
                    strip_target.samples(),
                )
                .unwrap()
            } else {
                strip_target.signature().unwrap()
            };
            let [x0, y0, x1, y1] = knobs.rect;
            let z = knobs.depth;
            let corners = [
                [x0, y0, z],
                [x1, y0, z],
                [x1, y1, z],
                [x0, y0, z],
                [x1, y1, z],
                [x0, y1, z],
            ];
            let input = ImportedVertexInput {
                buffer: common::vertex_buffer(context.device(), &corners.concat()),
                stride: 12,
                step: wgpu::VertexStepMode::Vertex,
                attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
            };
            let state = strip_target.render_state(raster);
            let draw = parts
                .runtime
                .prepare_imported_draw(
                    &ImportedDraw {
                        program: &parts.strip,
                        state: &state,
                        vertex_inputs: &[input],
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
                .map_err(|error| common::program_error("strip", &error))?;
            strip_target.admit("strip", &draw)?;
            Some(draw)
        };
        let imports: Vec<Import> = NAMES
            .iter()
            .zip(0..)
            .map(|(name, index)| {
                let texture = self.targets.texture(name)?;
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                Ok(Import::texture(
                    EngineSemantic::named(format!("strip.g{index}")),
                    texture,
                    view,
                ))
            })
            .collect::<Result<_, FrameError>>()?;
        let state = light_target.render_state(raster);
        let light = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.light,
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
                light_target.signature().unwrap(),
            )
            .map_err(|error| common::program_error("light", &error))?;
        light_target.admit("light", &light)?;
        Ok(Draws { strip, light })
    }
}

impl PreparedFeature for StripFrame {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.knobs.lock().unwrap().counts.prepares += 1;
        // The draws are prepared once, when the feature first sees its passes, and recorded in
        // every later frame: a steady state builds nothing.
        if self.parts.lock().unwrap().draws.is_none() {
            let draws = self.prepare_draws(context)?;
            self.parts.lock().unwrap().draws = Some(draws);
        }
        let stats = self.parts.lock().unwrap().runtime.stats();
        self.knobs.lock().unwrap().stats = Some(stats);
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        let draws = parts
            .draws
            .as_ref()
            .expect("prepared before the passes open");
        self.knobs
            .lock()
            .unwrap()
            .counts
            .recorded
            .push(context.entry);
        if context.entry == STRIP {
            draws
                .strip
                .as_ref()
                .expect("planned with the strip")
                .record(pass);
        } else {
            draws.light.record(pass);
        }
    }
}
