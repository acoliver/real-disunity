//! A game's pass samples the stock ambient occlusion of the frame it draws in, through the handle
//! its build is given, after declaring it as a frame read. The lent shadow cascades are tested in
//! `src/shadows/tests/game_shadows.rs` and `game_passes/shadow_cascades.rs`.
use super::common::{self, unfilterable_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use super::shadowed::effects_with;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver,
};
use worldrunner_render_program::binding::{EngineSemantic, ResourceLayout};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedResource, ImportedTexture,
    PreparedProgramDraw, ProgramRuntime, whole_range,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameCameraHandle, FrameEnvironment, FrameEnvironmentHandle, FrameInputError,
        FrameOcclusion, FrameOcclusionHandle, FrameShadowHandle, FrameShadows, GameBuild,
        GameFeatureFactory, GamePassSpec, GamePasses, OcclusionDepth, PassTarget,
    },
    executor::{
        AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
        GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
    },
    wgpu,
};

/// Green where the occlusion at the centre leaves the flat quad unoccluded, blue always.
const OCCLUSION_WGSL: &str = "
@group(0) @binding(0) var lent: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let centre = vec2<i32>(textureDimensions(lent) / 2u);
    let visibility = textureLoad(lent, centre, 0).r;
    return vec4<f32>(0.0, select(0.0, 1.0, visibility > 0.9), 1.0, 1.0);
}";

const SAMPLER: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);

/// What the game saw of its frame's inputs, in `plan`.
#[derive(Default)]
struct Seen {
    shadow: Vec<Result<FrameShadows, FrameInputError>>,
    occlusion: Vec<Result<Option<FrameOcclusion>, FrameInputError>>,
    environment: Vec<Result<Option<FrameEnvironment>, FrameInputError>>,
}

type Shared = Arc<Mutex<Seen>>;

struct Sampler {
    seen: Shared,
}

impl GameFeatureFactory for Sampler {
    fn name(&self) -> &'static str {
        "lighting sampler"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let runtime = ProgramRuntime::new(
            context.context.device().clone(),
            context.context.submission().clone(),
            AllocationBudget(1 << 24),
        );
        let binding = unfilterable_binding(0, "lent", wgpu::TextureViewDimension::D2);
        let program = common::graphics(
            &runtime,
            OCCLUSION_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: ResourceLayout::new(vec![binding]),
                constants: Vec::new(),
            },
        );
        Ok(Box::new(SamplerFeature {
            seen: self.seen.clone(),
            targets: context.targets.clone(),
            camera: context.camera.clone(),
            shadow: context.shadows.clone(),
            occlusion: context.occlusion.clone(),
            environment: context.environment.clone(),
            parts: Arc::new(Mutex::new(Parts {
                runtime,
                program,
                draw: None,
            })),
        }))
    }
}

struct Parts {
    runtime: ProgramRuntime,
    program: GraphicsProgram,
    draw: Option<PreparedProgramDraw>,
}

#[derive(Clone)]
struct SamplerFeature {
    seen: Shared,
    targets: worldrunner_render_scene::compositor::GameTargets,
    camera: FrameCameraHandle,
    shadow: FrameShadowHandle,
    occlusion: FrameOcclusionHandle,
    environment: FrameEnvironmentHandle,
    parts: Arc<Mutex<Parts>>,
}

impl SamplerFeature {
    /// The occlusion the pass samples.
    fn lent(&self) -> Result<ImportedTexture, FrameError> {
        let occlusion =
            self.occlusion
                .current()?
                .ok_or_else(|| FrameError::FeaturePreparation {
                    feature: "lighting sampler",
                    reason: "the frame lends no occlusion".to_owned(),
                })?;
        Ok(ImportedTexture {
            range: whole_range(&occlusion.texture),
            texture: occlusion.texture,
            view: occlusion.view,
            dimension: wgpu::TextureViewDimension::D2,
        })
    }
}

impl FrameFeature for SamplerFeature {
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
        {
            let mut seen = self.seen.lock().unwrap();
            seen.shadow.push(self.shadow.current());
            seen.occlusion.push(self.occlusion.current());
            seen.environment.push(self.environment.current());
        }
        self.lent()?;
        let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
            .writing([ColorOutput::Frame])
            .sampling_frame([FrameOcclusion::read()]);
        Ok(FeatureDeclaration {
            passes: vec![self.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for SamplerFeature {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let texture = self.lent()?;
        let target = PassTarget::of(context, SAMPLER)?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: self.camera.current()?.front_face,
            alpha_to_coverage: false,
        });
        let mut parts = self.parts.lock().unwrap();
        let parts = &mut *parts;
        let imports = [Import {
            semantic: EngineSemantic::named("lent"),
            resource: ImportedResource::Texture(texture),
        }];
        let draw = parts
            .runtime
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.program,
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
                target
                    .signature()
                    .map_err(|error| common::program_error("lighting sampler", &error))?,
            )
            .map_err(|error| common::program_error("lighting sampler", &error))?;
        target.admit("lighting sampler", &draw)?;
        parts.draw = Some(draw);
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let parts = self.parts.lock().unwrap();
        parts.draw.as_ref().expect("prepared").record(pass);
    }
}

fn inventory() -> (GamePasses, Shared) {
    let seen = Shared::default();
    let inventory = GamePasses::new().with(Arc::new(Sampler { seen: seen.clone() }));
    (inventory, seen)
}

#[test]
fn an_unshadowed_frame_lends_no_shadow_and_no_occlusion_it_does_not_compute() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = inventory();
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    let refusal = rig.render().unwrap_err();
    assert!(
        refusal.to_string().contains("the frame lends no occlusion"),
        "{refusal}"
    );
    let seen = seen.lock().unwrap();
    assert!(matches!(
        seen.shadow.last(),
        Some(Err(FrameInputError::NotPrepared))
    ));
    assert!(matches!(seen.occlusion.last(), Some(Ok(None))));
    assert!(
        matches!(seen.environment.last(), Some(Ok(None))),
        "every frame publishes the environment, none when no cube is installed"
    );
}

pub(super) fn ssao() -> SceneSsao {
    SceneSsao {
        receiver: SceneSsaoReceiver::Nearest,
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: 1_000_000,
        sample_count: 16,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}

#[test]
fn a_frame_with_ambient_occlusion_lends_the_filtered_occlusion_its_receivers_sample() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, seen) = inventory();
    let source = quad_scene();
    let scene = source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(effects_with(Some(ssao()))),
        )
        .unwrap();
    let mut rig = Rig::with_ssao(&scene, SIZE, ssao(), |settings| {
        settings.game_passes = inventory;
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    let image = rig.read_half(rig.renderer.targets().texture());
    let pixel = image[20 * SIZE[0] as usize + 3];
    assert_eq!(
        pixel.map(f32::to_bits),
        [0.0_f32, 1.0, 1.0, 1.0].map(f32::to_bits),
        "the pass ran after the occlusion filter and read an unoccluded centre: {pixel:?}"
    );
    let seen = seen.lock().unwrap();
    let occlusion = seen.occlusion.last().unwrap().clone().unwrap().unwrap();
    assert_eq!(occlusion.texture.format(), wgpu::TextureFormat::R32Float);
    assert_eq!(
        [occlusion.texture.width(), occlusion.texture.height()],
        SIZE
    );
    assert_eq!(occlusion.depth, OcclusionDepth::None);
    assert!(occlusion.stock_receivers);
    assert!(matches!(
        seen.shadow.last(),
        Some(Err(FrameInputError::NotPrepared))
    ));
}
