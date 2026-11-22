//! A game declares textures with whole mip chains, block-compressed or in arrays, one-texel
//! volumes, and a texture two factories share; the renderer uploads them through the scene
//! textures' upload plan and the game's passes sample every level.
use super::common::{self, array_binding, texture_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, ImportedDraw, ImportedGeometry, PreparedProgramDraw,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, GameBuffer, GameBufferDeclaration, GameBufferUsage, GameBuild,
        GameFeatureFactory, GamePassSpec, GamePasses, GameResourceError, GameResources,
        GameSampler, GameTargets, GameTexture, GameTextureDeclaration, PassTarget,
        TargetDeclaration, TargetExtent,
    },
    wgpu,
};

/// Every 4 columns of the target sample one level: the BC1 chain's four mips, the array's two
/// layers of three mips, the volume's texel and the shared texture's; the rest stay clear.
const WGSL: &str = "
@group(0) @binding(0) var bc: texture_2d<f32>;
@group(0) @binding(1) var layered: texture_2d_array<f32>;
@group(0) @binding(2) var volume: texture_3d<f32>;
@group(0) @binding(3) var common_texture: texture_2d<f32>;
@group(0) @binding(4) var nearest: sampler;
@vertex fn vs(@location(0) corner: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(corner, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let column = u32(at.x) / 4u;
    let uv = vec2<f32>(0.5, 0.5);
    if column < 4u {
        return textureSampleLevel(bc, nearest, uv, f32(column));
    }
    if column < 10u {
        let level = column - 4u;
        return textureSampleLevel(layered, nearest, uv, i32(level / 3u), f32(level % 3u));
    }
    if column == 10u {
        return textureSampleLevel(volume, nearest, vec3<f32>(0.5), 0.0);
    }
    if column == 11u {
        return textureSampleLevel(common_texture, nearest, uv, 0.0);
    }
    return vec4<f32>(0.0);
}";

/// The RGB565 colour of each BC1 mip, finest first: red, green, blue, white.
const BC_COLOURS: [u16; 4] = [0xF800, 0x07E0, 0x001F, 0xFFFF];
/// What sampling each BC1 mip returns.
const BC_TEXELS: [[u8; 4]; 4] = [
    [255, 0, 0, 255],
    [0, 255, 0, 255],
    [0, 0, 255, 255],
    [255, 255, 255, 255],
];
const VOLUME_TEXEL: [u8; 2] = [33, 99];
const SHARED_TEXEL: [u8; 4] = [10, 150, 250, 255];
const SHARED: &str = "mips.shared";
/// Vertices 24, BC1 chain 56, array 168, volume 2 and shared texture 16 bytes.
const DECLARED: u64 = 24 + 56 + 168 + 2 + 16;

/// The red of the array's `layer` at `mip`.
fn array_red(layer: u32, mip: u32) -> u8 {
    u8::try_from(40 * (3 * layer + mip) + 20).unwrap()
}

/// One solid BC1 block per level of an 8 x 8 chain: 2 x 2 blocks, then one block for each of
/// 4 x 4, 2 x 2 and 1 x 1. Both end colours are the same, so index 0 decodes to it exactly.
fn bc_chain() -> Vec<u8> {
    let block = |colour: u16| {
        let [low, high] = colour.to_le_bytes();
        [low, high, low, high, 0, 0, 0, 0]
    };
    let mut texels = Vec::new();
    for (mip, colour) in BC_COLOURS.into_iter().enumerate() {
        let blocks = if mip == 0 { 4 } else { 1 };
        for _ in 0..blocks {
            texels.extend(block(colour));
        }
    }
    texels
}

/// Two layers of a 4 x 4 chain of three levels, each level one solid colour.
fn array_chain(declaration: &GameTextureDeclaration<'_>) -> Vec<u8> {
    let mut texels = Vec::new();
    for level in declaration.levels().unwrap() {
        let texel = [array_red(level.layer, level.mip), 0, 0, 255];
        let count = usize::try_from(level.len).unwrap() / 4;
        texels.extend(texel.repeat(count));
    }
    texels
}

fn shared_declaration(texels: &[u8]) -> GameTextureDeclaration<'_> {
    GameTextureDeclaration {
        label: SHARED,
        format: wgpu::TextureFormat::Rgba8Unorm,
        size: [2, 2, 1],
        dimension: wgpu::TextureViewDimension::D2,
        mip_levels: 1,
        texels: Some(texels),
    }
}

struct Handles {
    vertices: GameBuffer,
    bc: GameTexture,
    array: GameTexture,
    volume: GameTexture,
    shared: GameTexture,
    nearest: GameSampler,
}

fn declare(resources: &GameResources) -> Result<Handles, GameResourceError> {
    let corners: Vec<u8> = [-1.0_f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let bc = bc_chain();
    let mut array = GameTextureDeclaration {
        label: "mips.array",
        format: wgpu::TextureFormat::Rgba8Unorm,
        size: [4, 4, 2],
        dimension: wgpu::TextureViewDimension::D2Array,
        mip_levels: 3,
        texels: None,
    };
    let array_texels = array_chain(&array);
    array.texels = Some(&array_texels);
    Ok(Handles {
        vertices: resources.declare_buffer(&GameBufferDeclaration {
            label: "mips.vertices",
            usage: GameBufferUsage::Vertex,
            contents: &corners,
        })?,
        bc: resources.declare_texture(&GameTextureDeclaration {
            label: "mips.bc",
            format: wgpu::TextureFormat::Bc1RgbaUnorm,
            size: [8, 8, 1],
            dimension: wgpu::TextureViewDimension::D2,
            mip_levels: 4,
            texels: Some(&bc),
        })?,
        array: resources.declare_texture(&array)?,
        volume: resources.declare_texture(&GameTextureDeclaration {
            label: "mips.volume",
            format: wgpu::TextureFormat::Rg8Unorm,
            size: [1, 1, 1],
            dimension: wgpu::TextureViewDimension::D3,
            mip_levels: 1,
            texels: Some(&VOLUME_TEXEL),
        })?,
        shared: resources.declare_shared_texture(&shared_declaration(&SHARED_TEXEL.repeat(4)))?,
        nearest: resources.declare_sampler(&wgpu::SamplerDescriptor::default()),
    })
}

fn semantic(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

fn program(resources: &GameResources) -> GraphicsProgram {
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let fragment = |binding, name, kind| ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source: BindingSource::Engine(semantic(name)),
        visibility: wgpu::ShaderStages::FRAGMENT,
        kind,
    };
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(vec![
            texture_binding(0, "mips.bc", filterable),
            array_binding(1, "mips.array"),
            fragment(
                2,
                "mips.volume",
                ResourceKind::Texture {
                    dimension: wgpu::TextureViewDimension::D3,
                    sample: filterable,
                    multisampled: false,
                },
            ),
            texture_binding(3, SHARED, filterable),
            fragment(
                4,
                "mips.nearest",
                ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ]),
        constants: Vec::new(),
    };
    common::graphics(
        &resources.runtime(),
        WGSL,
        "vs",
        "fs",
        StageDeclarations::default(),
        declarations,
    )
}

thread_local! {
    /// Each game's resources and shared texture handle, by its target.
    static KEPT: RefCell<Vec<(&'static str, GameResources, GameTexture)>> =
        const { RefCell::new(Vec::new()) };
}

/// Empties [`KEPT`] when dropped, also while a failed assertion unwinds, so the renderer's
/// resources it holds drop before the thread's other locals are destroyed.
struct ClearKept;

impl Drop for ClearKept {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(Vec::clear);
    }
}

fn kept(game: &'static str) -> (GameResources, GameTexture) {
    KEPT.with_borrow(|all| {
        let (_, resources, shared) = all.iter().find(|(name, ..)| *name == game).unwrap();
        (resources.clone(), *shared)
    })
}

struct MipGame {
    out: &'static str,
    occurrence: u32,
}

impl GameFeatureFactory for MipGame {
    fn name(&self) -> &'static str {
        self.out
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            TargetDeclaration::new(
                self.out,
                wgpu::TextureFormat::Rgba8Unorm,
                TargetExtent::Frame,
            )
            .with_usage(wgpu::TextureUsages::COPY_SRC),
        ]
    }

    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(1 << 16)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let resources = context.resources.clone();
        let handles = declare(&resources)?;
        KEPT.with_borrow_mut(|all| all.push((self.out, resources.clone(), handles.shared)));
        Ok(Box::new(Game(Rc::new(Parts {
            out: self.out,
            entry: PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), self.occurrence),
            targets: context.targets.clone(),
            program: program(&resources),
            resources,
            handles,
            draw: RefCell::new(None),
        }))))
    }
}

struct Parts {
    out: &'static str,
    entry: PassEntry,
    targets: GameTargets,
    resources: GameResources,
    handles: Handles,
    program: GraphicsProgram,
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
        let spec = GamePassSpec::new(GamePlace::BeforeOpaque, self.0.entry.occurrence())
            .writing([ColorOutput::Target(ResourceRef::new(self.0.out))]);
        Ok(FeatureDeclaration {
            passes: vec![self.0.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let parts = &self.0;
        if parts.draw.borrow().is_some() {
            return Ok(());
        }
        let resources = &parts.resources;
        let handles = &parts.handles;
        let imports = [
            resources.import_texture(semantic("mips.bc"), handles.bc)?,
            resources.import_texture(semantic("mips.array"), handles.array)?,
            resources.import_texture(semantic("mips.volume"), handles.volume)?,
            resources.import_texture(semantic(SHARED), handles.shared)?,
            resources.import_sampler(semantic("mips.nearest"), handles.nearest)?,
        ];
        let vertices = resources.vertex_input(
            handles.vertices,
            (8, wgpu::VertexStepMode::Vertex),
            wgpu::vertex_attr_array![0 => Float32x2].to_vec(),
        )?;
        let target = PassTarget::of(context, parts.entry)?;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("mip game", &error))?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        let draw = resources
            .runtime()
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &parts.program,
                    state: &state,
                    vertex_inputs: &[vertices],
                    bindings: &[],
                    imports: &imports,
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                signature,
            )
            .map_err(|error| common::program_error("mip game", &error))?;
        target.admit("mip game", &draw)?;
        *parts.draw.borrow_mut() = Some(draw);
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, self.0.entry);
        if let Some(draw) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

/// The texel each group of four columns should hold.
fn expected(column: u32) -> [u8; 4] {
    match column {
        0..4 => BC_TEXELS[usize::try_from(column).unwrap()],
        4..10 => [array_red((column - 4) / 3, (column - 4) % 3), 0, 0, 255],
        10 => [VOLUME_TEXEL[0], VOLUME_TEXEL[1], 0, 255],
        11 => SHARED_TEXEL,
        _ => [0; 4],
    }
}

fn check_output(rig: &Rig, out: &'static str) {
    let texture = rig.renderer.game_targets().texture(out).unwrap();
    let image = rig.read(&texture);
    for column in 0..SIZE[0] / 4 {
        let found = Rig::pixel(&image, usize::try_from(column * 4 + 2).unwrap(), 5);
        assert_eq!(found, expected(column), "{out}, column {column}");
    }
}

/// The bytes of each hold of the shared texture in the runtime ledger of `resources`.
fn shared_holds(resources: &GameResources) -> Vec<u64> {
    resources
        .runtime()
        .holdings()
        .into_iter()
        .filter(|holding| holding.label == SHARED)
        .flat_map(|holding| std::iter::repeat_n(holding.largest, holding.count))
        .collect()
}

/// Both stores hold the shared texture as a handle of their own and count it; another label's
/// declaration that differs, and a format the device cannot sample, are refused by name.
fn check_stores() {
    let (first, first_shared) = kept("mips.first");
    let (second, second_shared) = kept("mips.second");
    assert_eq!(first.bytes(), DECLARED);
    assert_eq!(
        second.bytes(),
        DECLARED,
        "each factory counts the shared texture"
    );
    let shared = semantic(SHARED);
    assert_eq!(
        first.import_texture(shared.clone(), first_shared),
        second.import_texture(shared.clone(), second_shared),
        "one texture behind both handles"
    );
    assert_eq!(
        first.import_texture(shared, second_shared),
        Err(GameResourceError::Foreign("game texture"))
    );
    for resources in [&first, &second] {
        assert_eq!(
            shared_holds(resources),
            vec![16],
            "each factory's runtime ledger holds the shared texture once"
        );
    }
    let texels = SHARED_TEXEL.repeat(4);
    assert_eq!(
        first.declare_shared_texture(&shared_declaration(&texels)),
        Ok(first_shared),
        "declaring it again is the same handle"
    );
    assert_eq!(first.bytes(), DECLARED);
    assert_eq!(
        shared_holds(&first),
        vec![16],
        "declaring it again holds nothing more"
    );
    let other = [0; 16];
    let refused = first.declare_shared_texture(&shared_declaration(&other));
    assert!(
        matches!(&refused, Err(GameResourceError::Declaration { label, reason })
            if label == SHARED && reason.contains("differs")),
        "{refused:?}"
    );
    let astc = wgpu::TextureFormat::Astc {
        block: wgpu::AstcBlock::B4x4,
        channel: wgpu::AstcChannel::Unorm,
    };
    let refused = first.declare_texture(&GameTextureDeclaration {
        label: "mips.astc",
        format: astc,
        size: [4, 4, 1],
        dimension: wgpu::TextureViewDimension::D2,
        mip_levels: 1,
        texels: Some(&[0; 16]),
    });
    assert!(
        matches!(&refused, Err(GameResourceError::Declaration { label, reason })
            if label == "mips.astc" && reason.contains("TEXTURE_COMPRESSION_ASTC")),
        "{refused:?}"
    );
}

#[test]
fn game_passes_sample_every_level_of_declared_chains_and_a_shared_texture() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let scene = quad_scene();
    let mut rig = Rig::with_features(
        &scene,
        SIZE,
        wgpu::Features::TEXTURE_COMPRESSION_BC,
        |settings| {
            settings.game_passes = GamePasses::new()
                .with(Arc::new(MipGame {
                    out: "mips.first",
                    occurrence: 0,
                }))
                .with(Arc::new(MipGame {
                    out: "mips.second",
                    occurrence: 1,
                }));
        },
    );
    rig.render().unwrap();
    check_output(&rig, "mips.first");
    check_output(&rig, "mips.second");
    check_stores();
}
