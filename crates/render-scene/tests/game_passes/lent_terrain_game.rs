//! The synthetic layered terrain and the game that asks the engine for its material arrays, for
//! `lent_terrain.rs` and the depth-prefix test.
use super::common::{self, array_binding};
use super::fixture::{Rig, SIZE, view};
use super::lent::{near, semantic};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::census::ResourceClass;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    MaterialHandle, RenderScene, SceneAlphaMode, SceneAppend, SceneGeometry, SceneMaterial,
    SceneMaterialParts, SceneNormalEncoding, SceneParts, SceneResources, SceneTerrain,
    SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture, TerrainLayer,
    TerrainLayeredSource, TerrainLodPolicy, TerrainSplatFilter, TerrainTypeGrid, TexelFormat,
    TextureAnisotropy, TextureColorSpace, TextureHandle,
};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::ResourceLayout;
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedResource, PreparedProgramDraw,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, EngineConstants, FULLSCREEN_TRIANGLE_WGSL, GameBuild, GameFeatureFactory,
        GamePassSpec, GamePasses, GameResources, GameTargets, LayerShape, PassTarget,
        TargetDeclaration, TargetExtent, TerrainMaterialArrayError, TerrainMaterialChannel,
    },
    draw::{PreparedAppend, SceneGpu},
    frame::FrameUniform,
    wgpu,
};

pub(super) const OUT: &str = "lent.terrain";
const ENTRY: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
pub(super) const CHANNELS: [TerrainMaterialChannel; 3] = [
    TerrainMaterialChannel::AlbedoMetallic,
    TerrainMaterialChannel::NormalRoughness,
    TerrainMaterialChannel::Height,
];
const LAYERS: usize = 4;
/// Heights of the terrain's 3 by 2 samples at bias zero, in millimetres, row by row from the
/// south.
const HEIGHTS: [i32; 6] = [0, 1500, -2500, 250, 12_345, -7];

// ---------------------------------------------------------------------------------------
// The scene: eight terrain surfaces whose maps are small solid textures.
// ---------------------------------------------------------------------------------------

/// The edge and mip count of the map of `channel` (0 albedo, 1 normal, 2 height) of `surface`.
/// Each special surface changes one map, because the layered terrain admits seven distinct
/// formats, extents and mip counts. Surface 4's albedo is larger, surface 5's normal and height
/// have one mip, and surface 7's height is larger with one mip, so each way of disagreeing with
/// the other surfaces is available.
fn shape_of(surface: u8, channel: usize) -> (u32, u32) {
    match (surface, channel) {
        (4, 0) => (4, 3),
        (5, 1 | 2) => (2, 1),
        (7, 2) => (4, 1),
        _ => (2, 2),
    }
}

/// The texel of `channel` (0 albedo, 1 normal, 2 height) of `surface` at mip `level`.
pub(super) fn colour(channel: usize, surface: u8, level: usize, bias: u8) -> [u8; 4] {
    let base = surface * 20 + bias * 7;
    let step = u8::try_from(level.min(1)).unwrap();
    match channel {
        0 => [base + 5 + 60 * step, 250 - base, 30 + 40 * step, 255],
        1 => [
            base + 3 + 50 * step,
            100 + base / 2,
            200 - base / 2 - 20 * step,
            255,
        ],
        _ => [base + 9 + 70 * step, 0, 0, 255],
    }
}

fn map(channel: usize, surface: u8, bias: u8) -> SceneTexture {
    let (edge, mips) = shape_of(surface, channel);
    let mut bytes = Vec::new();
    for level in 0..mips {
        let side = (edge >> level).max(1);
        for _ in 0..side * side {
            let texel = colour(channel, surface, level as usize, bias);
            if channel == 2 {
                bytes.push(texel[0]);
            } else {
                bytes.extend_from_slice(&texel);
            }
        }
    }
    SceneTexture::Texels {
        format: if channel == 2 {
            TexelFormat::R8Unorm
        } else {
            TexelFormat::Rgba8Unorm
        },
        // Surface 6 disagrees with the others in format alone.
        color_space: if channel == 0 && surface == 6 {
            TextureColorSpace::Srgb
        } else {
            TextureColorSpace::Linear
        },
        width: edge,
        height: edge,
        mips,
        bytes: bytes.into(),
    }
}

fn surface(index: u8) -> SceneTerrainSurface {
    let handle = |channel: u32| TextureHandle::from_raw(u32::from(index) * 3 + channel);
    SceneTerrainSurface::new(
        MaterialHandle::from_raw(0),
        SceneTerrainAppearance {
            layered: Some(TerrainLayer {
                albedo_metallic: handle(0),
                normal_roughness: handle(1),
                height: handle(2),
                normal_encoding: SceneNormalEncoding::Rgb,
                repeat_millimeters: 2000,
                offset_millimeters: [0; 2],
                roughness_metallic_offset_micro: [0; 2],
            }),
            ..SceneTerrainAppearance::NEUTRAL
        },
    )
}

/// The terrain of 3 by 2 samples and eight layered surfaces. `bias` changes every texel and
/// every height, so a second scene differs from the first in what a game reads.
pub(super) fn layered_scene(bias: u8) -> RenderScene {
    let textures = (0..8u8)
        .flat_map(|index| (0..3).map(move |channel| map(channel, index, bias)))
        .collect();
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let indices: Vec<u8> = (0..6).collect();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 3,
        rows: 2,
        origin_east_millimeters: -2000,
        origin_north_millimeters: -1000,
        sample_spacing_millimeters: 2000,
        heights_millimeters: HEIGHTS
            .iter()
            .map(|height| height + i32::from(bias) * 1000)
            .collect(),
        surface_indices: indices.clone(),
        surfaces: (0..8).map(surface).collect(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        anisotropy: TextureAnisotropy::default(),
        channel_map: None,
        type_grid: TerrainTypeGrid::new([3, 2], [-2000, -1000], 2000, indices).unwrap(),
        filter: TerrainSplatFilter::Bilinear,
        colorization: None,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material]).unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}

// ---------------------------------------------------------------------------------------
// The game.
// ---------------------------------------------------------------------------------------

/// The three arrays: 8 columns of 8 pixels, layers 0 to 3 at mip 0 then at mip 1, and a row of
/// 16 pixels for each channel (albedo, normal, height).
const ARRAYS_FRAGMENT: &str = "
@group(0) @binding(0) var albedo: texture_2d_array<f32>;
@group(0) @binding(1) var normal: texture_2d_array<f32>;
@group(0) @binding(2) var height: texture_2d_array<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return fullscreen_position(i);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let column = u32(at.x) / 8u;
    let layer = i32(column % 4u);
    let level = i32(column / 4u);
    let origin = vec2<i32>(0, 0);
    let row = u32(at.y) / 16u;
    if row == 0u { return textureLoad(albedo, origin, layer, level); }
    if row == 1u { return textureLoad(normal, origin, layer, level); }
    return vec4<f32>(textureLoad(height, origin, layer, level).r, 0.0, 0.0, 1.0);
}";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Ask {
    /// The three arrays of the script's layers, drawn as bands.
    Arrays,
    /// Every request of the script, recorded and not drawn.
    Refusals,
}

/// What the test tells the game to request.
#[derive(Default)]
pub(super) struct Script {
    pub(super) layers: Vec<u8>,
    /// Ask for the albedo array a second time in the frame, and keep both answers.
    pub(super) repeat: bool,
    /// Fail the frame's preparation after the arrays were asked for.
    pub(super) fail: bool,
    pub(super) requests: Vec<(TerrainMaterialChannel, Vec<u8>)>,
}

/// What the game saw that crosses threads.
#[derive(Default)]
pub(super) struct Seen {
    pub(super) refusals: Vec<Result<(), TerrainMaterialArrayError>>,
    pub(super) arrays: Vec<TerrainMaterialArrayError>,
}

/// What the game kept that does not cross threads.
#[derive(Default)]
pub(super) struct Kept {
    pub(super) engine: Option<EngineConstants>,
    /// The resources of each factory, in the order the renderer built them.
    pub(super) resources: Vec<GameResources>,
    /// The arrays of each frame, albedo, normal and height.
    pub(super) arrays: Vec<[wgpu::Texture; 3]>,
    /// The albedo array of each frame that asked twice, as both requests were answered.
    pub(super) twice: Vec<[wgpu::Texture; 2]>,
}

thread_local! {
    pub(super) static KEPT: RefCell<Kept> = RefCell::new(Kept::default());
}

/// Empties [`KEPT`] when dropped, so the renderer's resources it holds drop before the thread's
/// other locals do, also while a failed assertion unwinds.
pub(super) struct Clear;

impl Drop for Clear {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(|kept| *kept = Kept::default());
    }
}

pub(super) struct Factory {
    pub(super) name: &'static str,
    /// Whether this factory declares the output target; only one factory of a renderer may.
    pub(super) owns_target: bool,
    pub(super) budget: u64,
    pub(super) ask: Ask,
    pub(super) script: Arc<Mutex<Script>>,
    pub(super) seen: Arc<Mutex<Seen>>,
}

impl GameFeatureFactory for Factory {
    fn name(&self) -> &'static str {
        self.name
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        if !self.owns_target {
            return Vec::new();
        }
        vec![
            TargetDeclaration::new(OUT, wgpu::TextureFormat::Rgba8Unorm, TargetExtent::Frame)
                .with_usage(wgpu::TextureUsages::COPY_SRC),
        ]
    }

    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(self.budget)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        KEPT.with_borrow_mut(|kept| {
            kept.engine = Some(context.engine.clone());
            kept.resources.push(context.resources.clone());
        });
        Ok(Box::new(Game(Rc::new(Parts {
            ask: self.ask,
            targets: context.targets.clone(),
            resources: context.resources.clone(),
            engine: context.engine.clone(),
            program: program(&context.resources),
            script: self.script.clone(),
            seen: self.seen.clone(),
            draw: RefCell::new(None),
        }))))
    }
}

fn program(resources: &GameResources) -> GraphicsProgram {
    let runtime = resources.runtime();
    let layout = vec![
        array_binding(0, "lent.albedo_array"),
        array_binding(1, "lent.normal_array"),
        array_binding(2, "lent.height_array"),
    ];
    common::graphics(
        &runtime,
        &format!("{FULLSCREEN_TRIANGLE_WGSL}{ARRAYS_FRAGMENT}"),
        "vs",
        "fs",
        StageDeclarations::default(),
        StageDeclarations {
            resources: ResourceLayout::new(layout),
            constants: Vec::new(),
        },
    )
}

struct Parts {
    ask: Ask,
    targets: GameTargets,
    resources: GameResources,
    engine: EngineConstants,
    program: GraphicsProgram,
    script: Arc<Mutex<Script>>,
    seen: Arc<Mutex<Seen>>,
    draw: RefCell<Option<PreparedProgramDraw>>,
}

#[derive(Clone)]
struct Game(Rc<Parts>);

impl FrameFeature for Game {
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
        if self.0.ask == Ask::Refusals {
            return Ok(self.declare());
        }
        let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
            .writing([ColorOutput::Target(ResourceRef::new(OUT))]);
        Ok(FeatureDeclaration {
            passes: vec![self.0.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl Game {
    /// The arrays of the script's layers, or `None` when one was refused, which the game records.
    fn array_imports(&self, context: &mut PrepareContext<'_>) -> Option<Vec<Import>> {
        let parts = &self.0;
        let layers = parts.script.lock().unwrap().layers.clone();
        let names = [
            "lent.albedo_array",
            "lent.normal_array",
            "lent.height_array",
        ];
        let mut imports = Vec::new();
        for (channel, name) in CHANNELS.into_iter().zip(names) {
            match parts
                .engine
                .material_array(context, semantic(name), channel, &layers)
            {
                Ok(import) => imports.push(import),
                Err(error) => {
                    parts.seen.lock().unwrap().arrays.push(error);
                    return None;
                }
            }
        }
        let textures: Vec<_> = imports.iter().map(texture_of).collect();
        if parts.script.lock().unwrap().repeat {
            let name = semantic("lent.albedo_array");
            let channel = CHANNELS[0];
            let second = parts.engine.material_array(context, name, channel, &layers);
            let second = texture_of(&second.expect("the same request is answered again"));
            KEPT.with_borrow_mut(|kept| kept.twice.push([textures[0].clone(), second]));
        }
        KEPT.with_borrow_mut(|kept| kept.arrays.push(textures.try_into().unwrap()));
        Some(imports)
    }

    fn record_refusals(&self, context: &mut PrepareContext<'_>) {
        let parts = &self.0;
        let requests = std::mem::take(&mut parts.script.lock().unwrap().requests);
        let answers: Vec<_> = requests
            .iter()
            .map(|(channel, layers)| {
                parts
                    .engine
                    .material_array(context, semantic("lent.refused"), *channel, layers)
                    .map(|_| ())
            })
            .collect();
        parts.seen.lock().unwrap().refusals.extend(answers);
    }

    fn prepare_draw(
        &self,
        context: &PrepareContext<'_>,
        imports: &[Import],
    ) -> Result<PreparedProgramDraw, FrameError> {
        let parts = &self.0;
        let target = PassTarget::of(context, ENTRY)?;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("lent terrain", &error))?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        let draw = parts
            .resources
            .runtime()
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.program,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports,
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &EngineConstants::fullscreen_triangle(),
                    instances: 0..1,
                    owners: &[],
                },
                signature,
            )
            .map_err(|error| common::program_error("lent terrain", &error))?;
        target.admit("lent terrain", &draw)?;
        Ok(draw)
    }
}

fn texture_of(import: &Import) -> wgpu::Texture {
    match &import.resource {
        ImportedResource::Texture(texture) => texture.texture.clone(),
        _ => panic!("a texture import"),
    }
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let imports = match self.0.ask {
            Ask::Arrays => self.array_imports(context),
            Ask::Refusals => {
                self.record_refusals(context);
                return Ok(());
            }
        };
        if self.0.script.lock().unwrap().fail {
            return Err(FrameError::MissingPass(ENTRY));
        }
        // A game whose request was refused draws nothing; the frame goes on.
        if let Some(imports) = imports {
            *self.0.draw.borrow_mut() = Some(self.prepare_draw(context, &imports)?);
        }
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, ENTRY);
        if let Some(draw) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

pub(super) struct Run {
    pub(super) rig: Rig,
    pub(super) script: Arc<Mutex<Script>>,
    pub(super) seen: Arc<Mutex<Seen>>,
}

pub(super) fn run(scene: &RenderScene, ask: Ask) -> Run {
    run_with_budget(scene, ask, 1 << 20)
}

/// As [`run`], with a game resource budget of `budget` bytes.
pub(super) fn run_with_budget(scene: &RenderScene, ask: Ask, budget: u64) -> Run {
    let script = Arc::new(Mutex::new(Script::default()));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let factory = Factory {
        name: OUT,
        owns_target: true,
        budget,
        ask,
        script: script.clone(),
        seen: seen.clone(),
    };
    let rig = Rig::new(scene, SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(Arc::new(factory));
    });
    Run { rig, script, seen }
}

/// The arrays the last frame was lent, albedo, normal and height.
pub(super) fn last_arrays() -> [wgpu::Texture; 3] {
    KEPT.with_borrow(|kept| kept.arrays.last().expect("a frame lent arrays").clone())
}

impl Run {
    pub(super) fn texture_bytes(&self) -> u64 {
        self.rig
            .context
            .submission()
            .census()
            .snapshot()
            .get(ResourceClass::Texture)
    }

    pub(super) fn output(&self) -> Vec<[u8; 4]> {
        let texture = self.rig.renderer.game_targets().texture(OUT).unwrap();
        self.rig.read(&texture)
    }

    /// Replaces the resident scene, as a host that uploads a new one does.
    pub(super) fn replace_scene(&mut self, scene: &RenderScene) {
        let device = self.rig.context.device();
        self.rig.gpu = SceneGpu::upload(
            device,
            self.rig.context.submission(),
            PreparedScene::new(scene).unwrap(),
            &self.rig.pipelines,
            None,
        )
        .unwrap();
        let frame = FrameUniform::unlit(&view(SIZE));
        self.rig.gpu.set_frame(self.rig.context.queue(), frame);
    }

    pub(super) fn append_texture(&mut self) {
        let append = SceneAppend {
            textures: vec![map(0, 0, 0)],
            ..SceneAppend::default()
        };
        let prepared = PreparedAppend::new(self.rig.gpu.resource_base(), &append).unwrap();
        self.rig
            .gpu
            .append_prepared(
                self.rig.context.device(),
                self.rig.context.submission(),
                &self.rig.pipelines,
                prepared,
                &append,
            )
            .unwrap();
    }
}

/// Checks the bands of `image`: layer `i` of each channel's array, at both mips, is the texel
/// of surface `layers[i]`.
pub(super) fn assert_bands(image: &[[u8; 4]], layers: &[u8], bias: u8) {
    for channel in 0..3 {
        for (layer, surface) in layers.iter().enumerate() {
            for level in 0..2 {
                let found = Rig::pixel(image, (level * LAYERS + layer) * 8 + 4, channel * 16 + 8);
                let expected = colour(channel, *surface, level, bias);
                assert!(
                    near(found, expected, 0),
                    "channel {channel} layer {layer} mip {level} holds surface {surface}: \
                     {found:?} against {expected:?}"
                );
            }
        }
    }
}

pub(super) fn shape(format: wgpu::TextureFormat, edge: u32, mips: u32) -> LayerShape {
    LayerShape {
        format,
        extent: [edge, edge],
        mips,
    }
}
