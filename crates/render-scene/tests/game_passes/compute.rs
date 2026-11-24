//! A game compute pass between the game's render passes: a render pass at `BeforeOpaque` writes a
//! seed target, a compute pass at `AfterOpaque` reads it and builds a three-level chain in another
//! target, one dispatch per level (each reading level n-1 and writing level n of the same texture
//! through one-level imports), and a render pass at `AfterOpaqueSnapshots` samples every level.
//! A second compute pass writes seven levels of a texture in one dispatch through seven storage
//! images. A third writes two levels of layer 1 of a two-layer target through images the kernel
//! declares as arrays of 2D images, each a one-layer array import, beside a 2D import of layer 0.
//! Every level is read back and compared with its closed form.
use super::common::{self, unfilterable_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use super::pyramid_model::read_mip;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassSlot, PrepareContext, PreparedAccess, PreparedFeature,
};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{
    ComputeProgram, GraphicsProgram, Import, ImportedDispatch, ImportedDraw, ImportedGeometry,
    PreparedProgramDraw,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, GameBuild, GameComputeSpec, GameFeatureFactory, GameLevel, GamePassSpec,
        GamePasses, GameResources, GameTargets, LevelAccess, PassTarget, TargetDeclaration,
        TargetExtent,
    },
    wgpu,
};

const FLOAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const SEED: &str = "compute.seed";
const CHAIN: &str = "compute.chain";
const DIGEST: &str = "compute.digest";
const WIDE: &str = "compute.wide";
const ARRAY: &str = "compute.array";
const SEED_SIZE: u32 = 8;
const LEVELS: u32 = 3;
const WIDE_SIZE: u32 = 64;
const WIDE_LEVELS: u32 = 7;
const ARRAY_SIZE: u32 = 16;
/// The texel of every chain level the digest samples.
const PROBE: [u32; 2] = [1, 1];

const VERTEX_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}";

/// The seed: one plus the texel's column plus 16 times its row.
const SEED_WGSL: &str = "
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let p = floor(at.xy);
    return vec4<f32>(1.0 + p.x + 16.0 * p.y, 0.0, 0.0, 1.0);
}";

/// Digest texel i is level i of the chain at the probe texel.
const DIGEST_WGSL: &str = "
@group(0) @binding(0) var chain: texture_2d<f32>;
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let level = i32(floor(at.x));
    return vec4<f32>(textureLoad(chain, vec2<i32>(1, 1), level).r, 0.0, 0.0, 1.0);
}";

/// Level 0 of the chain: twice the seed.
const DOUBLE_WGSL: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<r32float, write>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    textureStore(destination, p, vec4<f32>(2.0 * textureLoad(source, p, 0).r, 0.0, 0.0, 0.0));
}";

/// Level n of the chain: the largest of the 2x2 block of level n-1 under the texel.
const REDUCE_WGSL: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<r32float, write>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<u32>(textureDimensions(destination));
    if (any(id.xy >= size)) { return; }
    let p = vec2<i32>(id.xy) * 2;
    let a = textureLoad(source, p, 0).r;
    let b = textureLoad(source, p + vec2<i32>(1, 0), 0).r;
    let c = textureLoad(source, p + vec2<i32>(0, 1), 0).r;
    let d = textureLoad(source, p + vec2<i32>(1, 1), 0).r;
    textureStore(destination, vec2<i32>(id.xy), vec4<f32>(max(max(a, b), max(c, d)), 0.0, 0.0, 0.0));
}";

/// Mips 0 and 1 of layer 1 of the array target through one-layer array images, each stored at
/// array index 0, and mip 0 of layer 0 through a 2D image: the texel's column plus 16 times its
/// row, plus 1000, 2000 and 3000.
const ARRAY_WGSL: &str = "
@group(0) @binding(0) var top: texture_storage_2d_array<r32float, write>;
@group(0) @binding(1) var next: texture_storage_2d_array<r32float, write>;
@group(0) @binding(2) var plain: texture_storage_2d<r32float, write>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    let v = f32(id.x + 16u * id.y);
    textureStore(top, p, 0, vec4<f32>(v + 1000.0, 0.0, 0.0, 0.0));
    if (all(id.xy < vec2<u32>(8u))) { textureStore(next, p, 0, vec4<f32>(v + 2000.0, 0.0, 0.0, 0.0)); }
    textureStore(plain, p, vec4<f32>(v + 3000.0, 0.0, 0.0, 0.0));
}";

/// Level m of the wide texture: the texel's column plus 64 times its row, plus m.
const SEVEN_WGSL: &str = "
@group(0) @binding(0) var m0: texture_storage_2d<r32float, write>;
@group(0) @binding(1) var m1: texture_storage_2d<r32float, write>;
@group(0) @binding(2) var m2: texture_storage_2d<r32float, write>;
@group(0) @binding(3) var m3: texture_storage_2d<r32float, write>;
@group(0) @binding(4) var m4: texture_storage_2d<r32float, write>;
@group(0) @binding(5) var m5: texture_storage_2d<r32float, write>;
@group(0) @binding(6) var m6: texture_storage_2d<r32float, write>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = vec2<i32>(id.xy);
    let v = vec4<f32>(f32(id.x + 64u * id.y), 0.0, 0.0, 0.0);
    textureStore(m0, p, v);
    if (all(id.xy < vec2<u32>(32u))) { textureStore(m1, p, v + 1.0); }
    if (all(id.xy < vec2<u32>(16u))) { textureStore(m2, p, v + 2.0); }
    if (all(id.xy < vec2<u32>(8u))) { textureStore(m3, p, v + 3.0); }
    if (all(id.xy < vec2<u32>(4u))) { textureStore(m4, p, v + 4.0); }
    if (all(id.xy < vec2<u32>(2u))) { textureStore(m5, p, v + 5.0); }
    if (all(id.xy < vec2<u32>(1u))) { textureStore(m6, p, v + 6.0); }
}";

fn target(name: &'static str, extent: [u32; 2], mips: u32) -> TargetDeclaration {
    TargetDeclaration::new(name, FLOAT, TargetExtent::Fixed(extent))
        .mips(mips)
        .with_usage(wgpu::TextureUsages::COPY_SRC)
}

fn storage(name: &'static str, side: u32, mips: u32) -> TargetDeclaration {
    target(name, [side, side], mips)
        .with_usage(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC)
}

fn seed() -> TargetDeclaration {
    target(SEED, [SEED_SIZE, SEED_SIZE], 1)
}

/// Persistent, so a submitted frame records the levels the compute pass wrote.
fn chain() -> TargetDeclaration {
    storage(CHAIN, SEED_SIZE, LEVELS).persistent()
}

fn digest() -> TargetDeclaration {
    target(DIGEST, [LEVELS, 1], 1)
}

fn wide() -> TargetDeclaration {
    storage(WIDE, WIDE_SIZE, WIDE_LEVELS)
}

/// Two layers of two mips: the kernel writes both mips of layer 1 and mip 0 of layer 0.
fn array() -> TargetDeclaration {
    storage(ARRAY, ARRAY_SIZE, 2).layers(2)
}

/// The levels of the array target the array pass writes, as (layer, mip).
const ARRAY_LEVELS: [(u32, u32); 3] = [(1, 0), (1, 1), (0, 0)];

fn writer() -> GamePassSpec {
    GamePassSpec::new(GamePlace::BeforeOpaque, 0).writing([ColorOutput::Target(seed().layer(0))])
}

fn chain_pass() -> GameComputeSpec {
    GameComputeSpec::new(GamePlace::AfterOpaque, 0)
        .reading([seed().layer(0)])
        .writing_mips(chain().layer_mips(0))
}

fn wide_pass() -> GameComputeSpec {
    GameComputeSpec::new(GamePlace::AfterOpaque, 1).writing_mips(wide().layer_mips(0))
}

fn array_pass() -> GameComputeSpec {
    let array = array();
    GameComputeSpec::new(GamePlace::AfterOpaque, 2).writing_mips(
        ARRAY_LEVELS
            .iter()
            .map(|&(layer, mip)| array.mip(layer, mip))
            .collect::<Vec<_>>(),
    )
}

fn reader() -> GamePassSpec {
    GamePassSpec::new(GamePlace::AfterOpaqueSnapshots, 0)
        .writing([ColorOutput::Target(digest().layer(0))])
        .reading([chain().layer(0)])
}

/// A small count as the `f32` that holds it exactly.
fn exact(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("a test count fits 16 bits"))
}

/// The chain's levels in row order, computed on the CPU.
fn model() -> Vec<Vec<f32>> {
    let side = SEED_SIZE as usize;
    let mut levels = vec![
        (0..side * side)
            .map(|index| 2.0 * (1.0 + exact(index % side) + 16.0 * exact(index / side)))
            .collect::<Vec<f32>>(),
    ];
    for level in 1..LEVELS as usize {
        let (above, side) = (&levels[level - 1], side >> level);
        let at = |x: usize, y: usize| above[y * (side * 2) + x];
        let next = (0..side * side)
            .map(|index| {
                let (x, y) = (2 * (index % side), 2 * (index / side));
                at(x, y)
                    .max(at(x + 1, y))
                    .max(at(x, y + 1))
                    .max(at(x + 1, y + 1))
            })
            .collect();
        levels.push(next);
    }
    levels
}

/// Mip `mip` of the array target's level written with `offset`, computed on the CPU.
fn array_model(mip: u32, offset: f32) -> Vec<f32> {
    let side = (ARRAY_SIZE >> mip) as usize;
    (0..side * side)
        .map(|index| exact(index % side) + 16.0 * exact(index / side) + offset)
        .collect()
}

/// Level `level` of the wide texture, computed on the CPU.
fn wide_model(level: u32) -> Vec<f32> {
    let side = (WIDE_SIZE >> level) as usize;
    (0..side * side)
        .map(|index| exact(index % side) + 64.0 * exact(index / side) + exact(level as usize))
        .collect()
}

/// A program whose bindings are storage images at `images` (arrays of 2D images at those also in
/// `arrays`) and sampled 2D textures elsewhere.
fn compute_program(
    context: &GameBuild<'_>,
    wgsl: &str,
    (images, arrays): (&[u32], &[u32]),
) -> ComputeProgram {
    let binding = |binding: u32, kind| ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source: BindingSource::Engine(EngineSemantic::named(format!("compute.{binding}"))),
        visibility: wgpu::ShaderStages::COMPUTE,
        kind,
    };
    let resources = (0..=images.iter().copied().max().unwrap_or(0))
        .map(|at| {
            if images.contains(&at) {
                binding(
                    at,
                    ResourceKind::StorageImage {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: FLOAT,
                        dimension: if arrays.contains(&at) {
                            wgpu::TextureViewDimension::D2Array
                        } else {
                            wgpu::TextureViewDimension::D2
                        },
                    },
                )
            } else {
                binding(
                    at,
                    ResourceKind::Texture {
                        dimension: wgpu::TextureViewDimension::D2,
                        sample: wgpu::TextureSampleType::Float { filterable: false },
                        multisampled: false,
                    },
                )
            }
        })
        .collect();
    let stage = StageProgram::new(
        StageModule {
            source: StageSource::Wgsl(Arc::from(wgsl)),
            entry_point: "main".to_owned(),
            kind: StageKind::Compute,
        },
        StageDeclarations {
            resources: ResourceLayout::new(resources),
            constants: Vec::new(),
        },
    )
    .unwrap();
    context.resources.runtime().compute_program(&stage).unwrap()
}

struct Programs {
    seed: GraphicsProgram,
    digest: GraphicsProgram,
    double: ComputeProgram,
    reduce: ComputeProgram,
    seven: ComputeProgram,
    array: ComputeProgram,
}

/// The compute pass a frame runs beside the chain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Extra {
    ChainOnly,
    SevenLevels,
    ArrayLevels,
}

struct Parts {
    targets: GameTargets,
    resources: GameResources,
    programs: Programs,
    /// The compute pass the frame also runs.
    extra: Extra,
    draws: RefCell<Vec<(GamePlace, PreparedProgramDraw)>>,
}

#[derive(Clone)]
struct Game(Rc<Parts>);

fn refused(error: &worldrunner_render_program::error::ProgramError) -> FrameError {
    common::program_error("compute", error)
}

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
        let targets = &self.0.targets;
        let mut declared = vec![
            targets.plan_pass(&writer(), passes, views)?,
            targets.plan_compute_pass(&chain_pass(), passes, views)?,
            targets.plan_pass(&reader(), passes, views)?,
        ];
        match self.0.extra {
            Extra::ChainOnly => {}
            Extra::SevenLevels => {
                declared.push(targets.plan_compute_pass(&wide_pass(), passes, views)?);
            }
            Extra::ArrayLevels => {
                declared.push(targets.plan_compute_pass(&array_pass(), passes, views)?);
            }
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

impl Game {
    fn level(
        &self,
        name: &'static str,
        binding: u32,
        mip: u32,
        access: LevelAccess,
    ) -> Result<Import, FrameError> {
        Ok(self.0.targets.import_level(
            name,
            EngineSemantic::named(format!("compute.{binding}")),
            GameLevel { layer: 0, mip },
            access,
        )?)
    }

    fn prepare_draw(
        &self,
        context: &PrepareContext<'_>,
        spec: &GamePassSpec,
        (program, imports): (&GraphicsProgram, &[Import]),
    ) -> Result<(), FrameError> {
        let target = PassTarget::of(context, spec.entry())?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        let draw = self
            .0
            .resources
            .runtime()
            .prepare_imported_draw(
                &ImportedDraw {
                    program,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports,
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                target.signature().map_err(|error| refused(&error))?,
            )
            .map_err(|error| refused(&error))?;
        target.admit("compute", &draw)?;
        self.0.draws.borrow_mut().push((spec.place, draw));
        Ok(())
    }

    /// Stages one dispatch of `program` over `imports` in `spec`'s pass.
    fn dispatch(
        &self,
        context: &mut PrepareContext<'_>,
        (spec, label): (&GameComputeSpec, &'static str),
        (program, imports, groups): (&ComputeProgram, &[Import], [u32; 3]),
        access: PreparedAccess<'_>,
    ) -> Result<(), FrameError> {
        let dispatch = self
            .0
            .resources
            .runtime()
            .prepare_imported_dispatch(&ImportedDispatch {
                program,
                bindings: &[],
                imports,
                groups,
                owners: &[],
            })
            .map_err(|error| refused(&error))?;
        context.record_in_compute_pass(spec.entry(), label, access, Box::new(dispatch))
    }

    /// The chain: level 0 from the seed, then each level from the one before.
    fn stage_chain(&self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let programs = &self.0.programs;
        let (seed, chain, pass) = (seed(), chain(), chain_pass());
        let imports = [
            self.level(SEED, 0, 0, LevelAccess::Sampled)?,
            self.level(CHAIN, 1, 0, LevelAccess::Storage)?,
        ];
        let access = PreparedAccess {
            reads: &[seed.layer(0)],
            writes: &[chain.mip(0, 0)],
        };
        self.dispatch(
            context,
            (&pass, "chain level 0"),
            (&programs.double, &imports, [1, 1, 1]),
            access,
        )?;
        for level in 1..LEVELS {
            let imports = [
                self.level(CHAIN, 0, level - 1, LevelAccess::Sampled)?,
                self.level(CHAIN, 1, level, LevelAccess::Storage)?,
            ];
            let access = PreparedAccess {
                reads: &[chain.mip(0, level - 1)],
                writes: &[chain.mip(0, level)],
            };
            self.dispatch(
                context,
                (&pass, "chain level"),
                (&programs.reduce, &imports, [1, 1, 1]),
                access,
            )?;
        }
        Ok(())
    }

    /// Every level of the wide texture in one dispatch through seven storage images.
    fn stage_wide(&self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let imports = (0..WIDE_LEVELS)
            .map(|level| self.level(WIDE, level, level, LevelAccess::Storage))
            .collect::<Result<Vec<_>, _>>()?;
        let writes: Vec<_> = wide().layer_mips(0).collect();
        let access = PreparedAccess {
            reads: &[],
            writes: &writes,
        };
        let groups = WIDE_SIZE / 8;
        self.dispatch(
            context,
            (&wide_pass(), "seven levels"),
            (&self.0.programs.seven, &imports, [groups, groups, 1]),
            access,
        )
    }
}

impl Game {
    /// Both mips of layer 1 of the array target through one-layer array imports and mip 0 of
    /// layer 0 through a 2D import, in one dispatch.
    fn stage_array(&self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let targets = &self.0.targets;
        let imports = ARRAY_LEVELS
            .iter()
            .zip(0..)
            .map(|(&(layer, mip), binding)| {
                let semantic = EngineSemantic::named(format!("compute.{binding}"));
                let level = GameLevel { layer, mip };
                if layer == 1 {
                    targets.import_level_array(ARRAY, semantic, level, LevelAccess::Storage)
                } else {
                    targets.import_level(ARRAY, semantic, level, LevelAccess::Storage)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let array = array();
        let writes: Vec<_> = ARRAY_LEVELS
            .iter()
            .map(|&(layer, mip)| array.mip(layer, mip))
            .collect();
        let access = PreparedAccess {
            reads: &[],
            writes: &writes,
        };
        let groups = ARRAY_SIZE / 8;
        self.dispatch(
            context,
            (&array_pass(), "array levels"),
            (&self.0.programs.array, &imports, [groups, groups, 1]),
            access,
        )
    }
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.0.draws.borrow_mut().clear();
        let programs = &self.0.programs;
        self.prepare_draw(context, &writer(), (&programs.seed, &[]))?;
        let whole = self
            .0
            .targets
            .import(CHAIN, EngineSemantic::named("compute.0"))?;
        self.prepare_draw(context, &reader(), (&programs.digest, &[whole]))?;
        self.stage_chain(context)?;
        match self.0.extra {
            Extra::ChainOnly => {}
            Extra::SevenLevels => self.stage_wide(context)?,
            Extra::ArrayLevels => self.stage_array(context)?,
        }
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        let draws = self.0.draws.borrow();
        let place = match context.slot {
            PassSlot::Game(place) => place,
            other => panic!("the game draws only in its own passes, not {other:?}"),
        };
        for (at, draw) in draws.iter() {
            if *at == place {
                draw.record(pass);
            }
        }
    }
}

struct ComputeGame {
    extra: Extra,
}

impl GameFeatureFactory for ComputeGame {
    fn name(&self) -> &'static str {
        "compute"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        let mut targets = vec![seed(), chain(), digest()];
        match self.extra {
            Extra::ChainOnly => {}
            Extra::SevenLevels => targets.push(wide()),
            Extra::ArrayLevels => targets.push(array()),
        }
        targets
    }

    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(1 << 20)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let runtime = context.resources.runtime();
        let digest_declarations = StageDeclarations {
            resources: ResourceLayout::new(vec![unfilterable_binding(
                0,
                "compute.0",
                wgpu::TextureViewDimension::D2,
            )]),
            constants: Vec::new(),
        };
        let graphics = |fragment: &str, declarations| {
            let source = format!("{VERTEX_WGSL}{fragment}");
            common::graphics(
                &runtime,
                &source,
                "vs",
                "fs",
                StageDeclarations::default(),
                declarations,
            )
        };
        let seed = graphics(SEED_WGSL, StageDeclarations::default());
        let digest = graphics(DIGEST_WGSL, digest_declarations);
        drop(runtime);
        let programs = Programs {
            seed,
            digest,
            double: compute_program(context, DOUBLE_WGSL, (&[1], &[])),
            reduce: compute_program(context, REDUCE_WGSL, (&[1], &[])),
            seven: compute_program(context, SEVEN_WGSL, (&[0, 1, 2, 3, 4, 5, 6], &[])),
            array: compute_program(context, ARRAY_WGSL, (&[0, 1, 2], &[0, 1])),
        };
        Ok(Box::new(Game(Rc::new(Parts {
            targets: context.targets.clone(),
            resources: context.resources.clone(),
            programs,
            extra: self.extra,
            draws: RefCell::new(Vec::new()),
        }))))
    }
}

fn rig(extra: Extra) -> Rig {
    let inventory = GamePasses::new().with(Arc::new(ComputeGame { extra }));
    Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    })
}

fn texture(rig: &Rig, name: &'static str) -> wgpu::Texture {
    rig.renderer.game_targets().texture(name).unwrap()
}

#[test]
fn a_compute_pass_between_two_render_passes_builds_a_chain_level_by_level() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut rig = rig(Extra::ChainOnly);
    let model = model();
    assert!(!rig.renderer.game_targets().is_mip_written(CHAIN, 0, 0));
    for frame in 0..2 {
        rig.render().unwrap();
        let chain = texture(&rig, CHAIN);
        for level in 0..LEVELS {
            assert!(
                rig.renderer.game_targets().is_mip_written(CHAIN, 0, level),
                "frame {frame}: a submitted frame records chain level {level} written"
            );
            assert_eq!(
                read_mip(&rig, &chain, (0, level)),
                model[level as usize],
                "frame {frame}: chain level {level}"
            );
        }
        let probe = |level: usize| {
            let side = (SEED_SIZE >> level) as usize;
            model[level][PROBE[1] as usize * side + PROBE[0] as usize]
        };
        assert_eq!(
            read_mip(&rig, &texture(&rig, DIGEST), (0, 0)),
            (0..LEVELS as usize).map(probe).collect::<Vec<_>>(),
            "frame {frame}: the later render pass samples every level"
        );
    }
}

#[test]
fn one_dispatch_writes_seven_levels_of_a_texture_through_seven_storage_images() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut rig = rig(Extra::SevenLevels);
    rig.render().unwrap();
    let wide = texture(&rig, WIDE);
    for level in 0..WIDE_LEVELS {
        assert_eq!(
            read_mip(&rig, &wide, (0, level)),
            wide_model(level),
            "wide level {level}"
        );
    }
}

#[test]
fn a_kernel_writes_levels_of_one_layer_through_images_it_declares_as_arrays() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut rig = rig(Extra::ArrayLevels);
    rig.render().unwrap();
    let array = texture(&rig, ARRAY);
    for (&(layer, mip), offset) in ARRAY_LEVELS.iter().zip([1000.0, 2000.0, 3000.0]) {
        assert_eq!(
            read_mip(&rig, &array, (layer, mip)),
            array_model(mip, offset),
            "layer {layer}, mip {mip}"
        );
    }
}
