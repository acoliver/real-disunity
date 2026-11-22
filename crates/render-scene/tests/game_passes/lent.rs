//! A game's passes draw with what the engine lends them: the resident scene meshes by handle, the
//! engine's constants (placeholder texels, the split-sum BRDF table, the full-screen triangle) and
//! the installed water normal map. The game creates no GPU resource of its own.
use super::common::{self, texture_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    MeshHandle, RenderScene, SceneAppend, SceneSpecularConvention, TextureHandle,
};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedResource, ImportedVertexInput,
    PreparedProgramDraw,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, EngineConstants, EnginePlaceholder, FULLSCREEN_TRIANGLE_WGSL, FrameInputError,
        FrameSceneMeshError, FrameSceneMeshes, FrameSceneMeshesHandle, FrameSceneTexturesHandle,
        FrameWaterNormalError, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses,
        GameResources, GameTargets, PassTarget, TargetDeclaration, TargetExtent,
    },
    draw::{PreparedAppend, PreparedMeshReplacement},
    wgpu,
};

const OUT: &str = "lent.out";
const ENTRY: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);
pub(super) const WATER_NORMAL: u32 = 1;

/// The stock quad's vertices through the stock layout (locations 0, 1 and 2), mapped to clip
/// space as the orthographic frame camera maps them: four metres to the top of the frame, and
/// the frame's aspect to its sides. The colour is the interpolated texture coordinate and the
/// normal's z.
const MESH_WGSL: &str = "
struct Varying {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
}
@vertex fn vs(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> Varying {
    var out: Varying;
    out.clip = vec4<f32>(position.x / 5.3333335, position.y / 4.0, 0.0, 1.0);
    out.uv = uv;
    out.normal = normal;
    return out;
}
@fragment fn fs(in: Varying) -> @location(0) vec4<f32> {
    return vec4<f32>(in.uv, in.normal.z, 1.0);
}";

/// Four vertical bands, 16 pixels each, of the frame: the white, black and flat-normal
/// placeholders and the BRDF table at texel (33, 20). The engine's triangle covers the frame.
const CONSTANTS_FRAGMENT: &str = "
@group(0) @binding(0) var white: texture_2d<f32>;
@group(0) @binding(1) var black: texture_2d<f32>;
@group(0) @binding(2) var flat_normal: texture_2d<f32>;
@group(0) @binding(3) var brdf: texture_2d<f32>;
@group(0) @binding(4) var point: sampler;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return fullscreen_position(i);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let centre = vec2<f32>(0.5, 0.5);
    let band = u32(at.x) / 16u;
    if band == 0u { return textureSampleLevel(white, point, centre, 0.0); }
    if band == 1u { return textureSampleLevel(black, point, centre, 0.0); }
    if band == 2u { return textureSampleLevel(flat_normal, point, centre, 0.0); }
    let table = textureSampleLevel(brdf, point, vec2<f32>(33.5, 20.5) / 64.0, 0.0);
    return vec4<f32>(table.r, table.g, 0.0, 1.0);
}";

/// Four vertical bands, 16 pixels each, of the zero placeholders: `R16Float`, `R8Unorm`,
/// `Rg8Unorm` and the 3D `Rg8Unorm`. Each band is `(0.5 + texel.r + texel.g, G, 0, 1)` with a
/// green `G` that tells the bands apart, so a texel that is not zero moves the red channel.
const ZEROS_FRAGMENT: &str = "
@group(0) @binding(0) var half_zero: texture_2d<f32>;
@group(0) @binding(1) var mask_zero: texture_2d<f32>;
@group(0) @binding(2) var pair_zero: texture_2d<f32>;
@group(0) @binding(3) var volume_zero: texture_3d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return fullscreen_position(i);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let band = u32(at.x) / 16u;
    var texel = textureLoad(half_zero, vec2<i32>(0, 0), 0);
    var green = 0.25;
    if band == 1u { texel = textureLoad(mask_zero, vec2<i32>(0, 0), 0); green = 0.5; }
    if band == 2u { texel = textureLoad(pair_zero, vec2<i32>(0, 0), 0); green = 0.75; }
    if band == 3u { texel = textureLoad(volume_zero, vec3<i32>(0, 0, 0), 0); green = 1.0; }
    return vec4<f32>(0.5 + texel.r + texel.g, green, 0.0, 1.0);
}";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scenario {
    Mesh,
    /// The second scene mesh, past another in the chunks both are packed into.
    Chunk,
    Constants,
    Water,
    Zeros,
}

/// What the games saw that crosses threads.
#[derive(Default)]
pub(super) struct Seen {
    pub(super) refusals: Vec<FrameSceneMeshError>,
    /// Per frame and body, whether the lent water normal map is the resident texture.
    pub(super) water: Vec<Result<bool, FrameWaterNormalError>>,
    pub(super) lent: Vec<String>,
    /// What a depth-prefix feature was answered for a terrain material array.
    pub(super) terrain: Vec<String>,
}

pub(super) type Shared = Arc<Mutex<Seen>>;

/// What the game saw that does not cross threads.
#[derive(Default)]
pub(super) struct Kept {
    pub(super) snapshots: Vec<FrameSceneMeshes>,
    pub(super) meshes: Option<FrameSceneMeshesHandle>,
    pub(super) textures: Option<FrameSceneTexturesHandle>,
    pub(super) brdf: Option<wgpu::Texture>,
    /// The zero placeholders each frame was lent, in the order of the bands.
    pub(super) zeros: Vec<[wgpu::Texture; 4]>,
}

thread_local! {
    pub(super) static KEPT: RefCell<Kept> = RefCell::new(Kept::default());
}

/// Empties [`KEPT`] when dropped, also while a failed assertion unwinds, so the renderer's
/// resources it holds drop before the thread's other locals are destroyed.
pub(super) struct ClearKept;

impl Drop for ClearKept {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(|kept| *kept = Kept::default());
    }
}

pub(super) fn semantic(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

struct LentGame {
    scenario: Scenario,
    seen: Shared,
}

fn constants_layout() -> ResourceLayout {
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    ResourceLayout::new(vec![
        texture_binding(0, "lent.white", filterable),
        texture_binding(1, "lent.black", filterable),
        texture_binding(2, "lent.flat_normal", filterable),
        texture_binding(3, "lent.brdf", filterable),
        ResourceBinding {
            at: BindingAddress { set: 0, binding: 4 },
            source: BindingSource::Engine(semantic("lent.point")),
            visibility: wgpu::ShaderStages::FRAGMENT,
            kind: ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
        },
    ])
}

fn zeros_layout() -> ResourceLayout {
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let volume = ResourceBinding {
        at: BindingAddress { set: 0, binding: 3 },
        source: BindingSource::Engine(semantic("lent.zero_volume")),
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind: ResourceKind::Texture {
            dimension: wgpu::TextureViewDimension::D3,
            sample: filterable,
            multisampled: false,
        },
    };
    ResourceLayout::new(vec![
        texture_binding(0, "lent.zero_half", filterable),
        texture_binding(1, "lent.zero_mask", filterable),
        texture_binding(2, "lent.zero_pair", filterable),
        volume,
    ])
}

fn program(resources: &GameResources, scenario: Scenario) -> GraphicsProgram {
    let runtime = resources.runtime();
    match scenario {
        Scenario::Mesh | Scenario::Chunk => common::graphics(
            &runtime,
            MESH_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations::default(),
        ),
        Scenario::Zeros => common::graphics(
            &runtime,
            &format!("{FULLSCREEN_TRIANGLE_WGSL}{ZEROS_FRAGMENT}"),
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: zeros_layout(),
                constants: Vec::new(),
            },
        ),
        Scenario::Constants | Scenario::Water => common::graphics(
            &runtime,
            &format!("{FULLSCREEN_TRIANGLE_WGSL}{CONSTANTS_FRAGMENT}"),
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: constants_layout(),
                constants: Vec::new(),
            },
        ),
    }
}

impl GameFeatureFactory for LentGame {
    fn name(&self) -> &'static str {
        OUT
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            TargetDeclaration::new(OUT, wgpu::TextureFormat::Rgba8Unorm, TargetExtent::Frame)
                .with_usage(wgpu::TextureUsages::COPY_SRC),
        ]
    }

    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(1 << 20)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        KEPT.with_borrow_mut(|kept| {
            kept.meshes = Some(context.scene_meshes.clone());
            kept.textures = Some(context.scene_textures.clone());
        });
        let point = context
            .resources
            .declare_sampler(&wgpu::SamplerDescriptor::default());
        Ok(Box::new(Game(Rc::new(Parts {
            scenario: self.scenario,
            targets: context.targets.clone(),
            resources: context.resources.clone(),
            engine: context.engine.clone(),
            meshes: context.scene_meshes.clone(),
            textures: context.scene_textures.clone(),
            program: program(&context.resources, self.scenario),
            point,
            seen: self.seen.clone(),
            draw: RefCell::new(None),
        }))))
    }
}

struct Parts {
    scenario: Scenario,
    targets: GameTargets,
    resources: GameResources,
    engine: EngineConstants,
    meshes: FrameSceneMeshesHandle,
    textures: FrameSceneTexturesHandle,
    program: GraphicsProgram,
    point: worldrunner_render_scene::compositor::GameSampler,
    seen: Shared,
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
        if self.0.scenario == Scenario::Water {
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

/// What a draw is made of.
struct DrawInputs {
    imports: Vec<Import>,
    vertex_inputs: Vec<ImportedVertexInput>,
    geometry: ImportedGeometry,
}

impl Game {
    fn mesh_inputs(&self) -> Result<DrawInputs, FrameError> {
        let parts = &self.0;
        let meshes = parts.meshes.current()?;
        let quad = MeshHandle::from_raw(0);
        let mut seen = parts.seen.lock().unwrap();
        if seen.refusals.is_empty() {
            seen.refusals.extend([
                meshes.vertex_input(MeshHandle::from_raw(7), 0).unwrap_err(),
                meshes
                    .indices(MeshHandle::from_raw(7), 0, 0..3)
                    .unwrap_err(),
                meshes.vertex_input(quad, 4).unwrap_err(),
                meshes.indices(quad, 0, 3..9).unwrap_err(),
            ]);
        }
        drop(seen);
        let (mesh, range) = match parts.scenario {
            Scenario::Chunk => (MeshHandle::from_raw(1), 3..6),
            _ => (quad, 0..meshes.index_count(quad, 0)?),
        };
        let inputs = DrawInputs {
            imports: Vec::new(),
            vertex_inputs: vec![meshes.vertex_input(mesh, 0)?],
            geometry: ImportedGeometry::Indexed(meshes.indices(mesh, 0, range)?),
        };
        KEPT.with_borrow_mut(|kept| kept.snapshots.push(meshes));
        Ok(inputs)
    }

    fn constants_inputs(&self, context: &mut PrepareContext<'_>) -> Result<DrawInputs, FrameError> {
        let parts = &self.0;
        let engine = &parts.engine;
        let brdf = engine.brdf_table(
            context,
            semantic("lent.brdf"),
            SceneSpecularConvention::default(),
        );
        if let ImportedResource::Texture(table) = &brdf.resource {
            KEPT.with_borrow_mut(|kept| kept.brdf = Some(table.texture.clone()));
        }
        Ok(DrawInputs {
            imports: vec![
                engine.placeholder(semantic("lent.white"), EnginePlaceholder::White),
                engine.placeholder(semantic("lent.black"), EnginePlaceholder::Black),
                engine.placeholder(semantic("lent.flat_normal"), EnginePlaceholder::FlatNormal),
                brdf,
                parts
                    .resources
                    .import_sampler(semantic("lent.point"), parts.point)?,
            ],
            vertex_inputs: Vec::new(),
            geometry: EngineConstants::fullscreen_triangle(),
        })
    }

    fn zeros_inputs(&self) -> DrawInputs {
        let engine = &self.0.engine;
        let imports = vec![
            engine.placeholder(semantic("lent.zero_half"), EnginePlaceholder::ZeroR16Float),
            engine.placeholder(semantic("lent.zero_mask"), EnginePlaceholder::ZeroR8),
            engine.placeholder(semantic("lent.zero_pair"), EnginePlaceholder::ZeroRg8),
            engine.placeholder(
                semantic("lent.zero_volume"),
                EnginePlaceholder::ZeroRg8Volume,
            ),
        ];
        let textures = imports.iter().map(|import| match &import.resource {
            ImportedResource::Texture(texture) => texture.texture.clone(),
            _ => panic!("a texture import"),
        });
        let textures: [wgpu::Texture; 4] = textures.collect::<Vec<_>>().try_into().unwrap();
        KEPT.with_borrow_mut(|kept| kept.zeros.push(textures));
        DrawInputs {
            imports,
            vertex_inputs: Vec::new(),
            geometry: EngineConstants::fullscreen_triangle(),
        }
    }

    fn record_water(&self) -> Result<(), FrameError> {
        let parts = &self.0;
        let textures = parts.textures.current()?;
        let resident = textures.import(semantic("lent.water"), TextureHandle::from_raw(1))?;
        let resident = match resident.resource {
            ImportedResource::Texture(texture) => texture.texture,
            _ => panic!("a texture import"),
        };
        let found = (0..3)
            .map(|body| {
                textures
                    .water_normal_map(semantic("lent.water"), body)
                    .map(|lent| {
                        matches!(
                            lent.resource,
                            ImportedResource::Texture(ref texture) if texture.texture == resident
                        )
                    })
            })
            .collect::<Vec<_>>();
        parts.seen.lock().unwrap().water.extend(found);
        Ok(())
    }

    fn prepare_draw(
        &self,
        context: &PrepareContext<'_>,
        inputs: &DrawInputs,
    ) -> Result<PreparedProgramDraw, FrameError> {
        let parts = &self.0;
        let target = PassTarget::of(context, ENTRY)?;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("lent game", &error))?;
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
                    vertex_inputs: &inputs.vertex_inputs,
                    bindings: &[],
                    imports: &inputs.imports,
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &inputs.geometry,
                    instances: 0..1,
                    owners: &[],
                },
                signature,
            )
            .map_err(|error| common::program_error("lent game", &error))?;
        target.admit("lent game", &draw)?;
        Ok(draw)
    }
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let inputs = match self.0.scenario {
            Scenario::Mesh | Scenario::Chunk => self.mesh_inputs()?,
            Scenario::Constants => self.constants_inputs(context)?,
            Scenario::Zeros => self.zeros_inputs(),
            Scenario::Water => return self.record_water(),
        };
        let draw = self.prepare_draw(context, &inputs)?;
        *self.0.draw.borrow_mut() = Some(draw);
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, ENTRY);
        if let Some(draw) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

pub(super) fn inventory(scenario: Scenario) -> (GamePasses, Shared) {
    let seen = Shared::default();
    let game = LentGame {
        scenario,
        seen: seen.clone(),
    };
    (GamePasses::new().with(Arc::new(game)), seen)
}

pub(super) fn output(rig: &Rig) -> Vec<[u8; 4]> {
    let texture = rig.renderer.game_targets().texture(OUT).unwrap();
    rig.read(&texture)
}

pub(super) fn near(found: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
    found
        .iter()
        .zip(expected)
        .all(|(found, expected)| found.abs_diff(expected) <= tolerance)
}

#[test]
fn a_game_pass_draws_a_resident_scene_mesh_through_the_lent_import() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let mut stock = Rig::new(&quad_scene(), SIZE, |_| {});
    let background = stock.render().unwrap()[0];
    let stock_covered: Vec<bool> = stock
        .render()
        .unwrap()
        .iter()
        .map(|texel| *texel != background)
        .collect();
    drop(stock);

    let (game_passes, _) = inventory(Scenario::Mesh);
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = game_passes;
    });
    rig.render().unwrap();
    let image = output(&rig);
    let covered: Vec<bool> = image.iter().map(|texel| texel[3] == 255).collect();
    assert_eq!(
        covered.iter().filter(|covered| **covered).count(),
        36 * 36,
        "the quad is 36 pixels on a side"
    );
    assert_eq!(
        covered, stock_covered,
        "the game pass covers what the stock draw covers"
    );
    // Column 20, row 10: u = (20.5 - 14) / 36, v from the top of the quad at row 6.
    let texel = Rig::pixel(&image, 20, 10);
    assert!(
        near(texel, [46, 223, 255, 255], 2),
        "the lent vertices carry the stock texture coordinates and normals: {texel:?}"
    );
}

/// The quad again, as a second mesh of an append.
fn append_of_the_quad(scene: &RenderScene) -> SceneAppend {
    SceneAppend {
        meshes: vec![scene.geometry().meshes()[0].clone()],
        ..SceneAppend::default()
    }
}

#[test]
fn the_lent_meshes_change_snapshot_on_append_and_replacement_and_refuse_by_type() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let (game_passes, seen) = inventory(Scenario::Mesh);
    let scene = quad_scene();
    let mut rig = Rig::new(&scene, SIZE, |settings| {
        settings.game_passes = game_passes;
    });
    rig.render().unwrap();
    rig.render().unwrap();
    let append = append_of_the_quad(&scene);
    let prepared = PreparedAppend::new(rig.gpu.resource_base(), &append).unwrap();
    rig.gpu
        .append_prepared(
            rig.context.device(),
            rig.context.submission(),
            &rig.pipelines,
            prepared,
            &append,
        )
        .unwrap();
    rig.render().unwrap();
    let replacement = PreparedMeshReplacement::new(
        &scene,
        MeshHandle::from_raw(0),
        &scene.geometry().meshes()[0],
    )
    .unwrap();
    rig.gpu
        .replace_mesh_prepared(rig.context.device(), replacement)
        .unwrap();
    rig.render().unwrap();

    KEPT.with_borrow(|kept| {
        let [first, second, appended, replaced] = kept.snapshots.as_slice() else {
            panic!("four frames lent four snapshots: {}", kept.snapshots.len());
        };
        assert!(
            first.same_snapshot(second),
            "an unchanged table keeps its snapshot"
        );
        assert!(!second.same_snapshot(appended), "an append makes a new one");
        assert!(
            !appended.same_snapshot(replaced),
            "a replacement makes a new one"
        );
        assert_eq!((first.len(), appended.len()), (1, 2));
        let quad = MeshHandle::from_raw(0);
        assert_ne!(
            appended.vertex_input(quad, 0).unwrap().buffer,
            replaced.vertex_input(quad, 0).unwrap().buffer,
            "the replaced mesh is lent from its new buffers"
        );
        assert!(appended.vertex_input(MeshHandle::from_raw(1), 0).is_ok());
        assert_eq!(
            kept.meshes.as_ref().unwrap().current().unwrap_err(),
            FrameInputError::OutsideFrame,
            "nothing is lent outside a frame"
        );
    });
    assert_eq!(
        seen.lock().unwrap().refusals,
        [
            FrameSceneMeshError::Unknown {
                handle: 7,
                resident: 1
            },
            FrameSceneMeshError::Unknown {
                handle: 7,
                resident: 1
            },
            FrameSceneMeshError::UnknownLevel {
                mesh: 0,
                level: 4,
                levels: 1
            },
            FrameSceneMeshError::IndexRange {
                mesh: 0,
                level: 0,
                range: 3..9,
                count: 6
            },
        ]
    );
}
