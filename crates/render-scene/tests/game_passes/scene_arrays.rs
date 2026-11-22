//! A game pass reads a lent scene texture as a 2D texture array, as a program written for
//! texture arrays declares it: `FrameSceneTextures::import_array` views the resident texture with
//! all its mips as a one-layer array, and the plain 2D import is refused at that binding.
use super::common::{self, array_binding};
use super::fixture::{Rig, SIZE, quad_scene_with};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureHandle};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, PreparedProgramDraw,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameSceneTextureError, FrameSceneTextures, FrameSceneTexturesHandle,
        GameBuffer, GameBufferDeclaration, GameBufferUsage, GameBuild, GameFeatureFactory,
        GamePassSpec, GamePasses, GameResources, GameSampler, GameTargets, PassTarget,
        TargetDeclaration, TargetExtent,
    },
    wgpu,
};

const OUT: &str = "arrays.out";
const ARRAY: &str = "arrays.scene";
/// The left half of the target samples mip 0 of layer 0, the right half mip 1.
const WGSL: &str = "
@group(0) @binding(0) var scene: texture_2d_array<f32>;
@group(0) @binding(1) var nearest: sampler;
@vertex fn vs(@location(0) corner: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(corner, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let level = select(0.0, 1.0, at.x >= 32.0);
    return textureSampleLevel(scene, nearest, vec2<f32>(0.5, 0.5), 0, level);
}";
/// Each mip of the scene texture is one colour whose sRGB code decodes exactly: red, then blue.
const MIP_TEXELS: [[u8; 4]; 2] = [[255, 0, 0, 255], [0, 0, 255, 255]];

/// A 2 x 2 sRGB albedo of two mips.
fn two_mips() -> SceneTexture {
    let mut bytes = MIP_TEXELS[0].repeat(4);
    bytes.extend(MIP_TEXELS[1]);
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 2,
        bytes: bytes.into(),
    }
}

fn semantic(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

/// What the game met besides its draw: the plain import refused, and the unknown handle.
#[derive(Default)]
struct Refusals {
    plain: Option<ProgramError>,
    unknown: Option<FrameSceneTextureError>,
}

struct ArrayGame {
    refusals: Arc<Mutex<Refusals>>,
}

impl GameFeatureFactory for ArrayGame {
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
        AllocationBudget(1 << 12)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let resources = context.resources.clone();
        let corners: Vec<u8> = [-1.0_f32, -1.0, 3.0, -1.0, -1.0, 3.0]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let vertices = resources.declare_buffer(&GameBufferDeclaration {
            label: "arrays.vertices",
            usage: GameBufferUsage::Vertex,
            contents: &corners,
        })?;
        let nearest = resources.declare_sampler(&wgpu::SamplerDescriptor::default());
        Ok(Box::new(Game(Rc::new(Parts {
            entry: PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0),
            targets: context.targets.clone(),
            scene: context.scene_textures.clone(),
            program: program(&resources),
            resources,
            vertices,
            nearest,
            refusals: self.refusals.clone(),
            draw: RefCell::new(None),
        }))))
    }
}

fn program(resources: &GameResources) -> GraphicsProgram {
    let fragment = StageDeclarations {
        resources: ResourceLayout::new(vec![
            array_binding(0, ARRAY),
            ResourceBinding {
                at: BindingAddress { set: 0, binding: 1 },
                source: BindingSource::Engine(semantic("arrays.nearest")),
                visibility: wgpu::ShaderStages::FRAGMENT,
                kind: ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
            },
        ]),
        constants: Vec::new(),
    };
    common::graphics(
        &resources.runtime(),
        WGSL,
        "vs",
        "fs",
        StageDeclarations::default(),
        fragment,
    )
}

struct Parts {
    entry: PassEntry,
    targets: GameTargets,
    scene: FrameSceneTexturesHandle,
    resources: GameResources,
    vertices: GameBuffer,
    nearest: GameSampler,
    program: GraphicsProgram,
    refusals: Arc<Mutex<Refusals>>,
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
    fn prepare_draw(
        &self,
        context: &PrepareContext<'_>,
        scene: Import,
    ) -> Result<PreparedProgramDraw, ProgramError> {
        let parts = &self.0;
        let resources = &parts.resources;
        let imports = [
            scene,
            resources
                .import_sampler(semantic("arrays.nearest"), parts.nearest)
                .unwrap(),
        ];
        let vertices = resources
            .vertex_input(
                parts.vertices,
                (8, wgpu::VertexStepMode::Vertex),
                wgpu::vertex_attr_array![0 => Float32x2].to_vec(),
            )
            .unwrap();
        let target = PassTarget::of(context, parts.entry).unwrap();
        let signature = target.signature()?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        resources.runtime().prepare_imported_draw(
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
    }

    /// Records how the 2D import and a handle past the table are refused.
    fn probe(&self, context: &PrepareContext<'_>, textures: &FrameSceneTextures) {
        let plain = textures
            .import(semantic(ARRAY), TextureHandle::from_raw(0))
            .unwrap();
        let mut refusals = self.0.refusals.lock().unwrap();
        refusals.plain = self.prepare_draw(context, plain).err();
        refusals.unknown = textures
            .import_array(semantic(ARRAY), TextureHandle::from_raw(5))
            .err();
    }
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        if self.0.draw.borrow().is_some() {
            return Ok(());
        }
        let textures = self.0.scene.current()?;
        self.probe(context, &textures);
        let array = textures.import_array(semantic(ARRAY), TextureHandle::from_raw(0))?;
        let draw = self
            .prepare_draw(context, array)
            .map_err(|error| common::program_error("array game", &error))?;
        PassTarget::of(context, self.0.entry)?.admit("array game", &draw)?;
        *self.0.draw.borrow_mut() = Some(draw);
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, self.0.entry);
        if let Some(draw) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

#[test]
fn a_game_pass_samples_every_mip_of_a_lent_scene_texture_as_an_array() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let refusals = Arc::new(Mutex::new(Refusals::default()));
    let scene = quad_scene_with(two_mips());
    let mut rig = Rig::new(&scene, SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(Arc::new(ArrayGame {
            refusals: refusals.clone(),
        }));
    });
    rig.render().unwrap();
    let texture = rig.renderer.game_targets().texture(OUT).unwrap();
    let image = rig.read(&texture);
    assert_eq!(Rig::pixel(&image, 5, 7), MIP_TEXELS[0], "mip 0 of layer 0");
    assert_eq!(Rig::pixel(&image, 50, 7), MIP_TEXELS[1], "mip 1 of layer 0");

    let refusals = refusals.lock().unwrap();
    assert!(
        matches!(
            &refusals.plain,
            Some(ProgramError::WrongResource {
                set: 0,
                binding: 0,
                ..
            })
        ),
        "the 2D view is refused at the array binding: {:?}",
        refusals.plain
    );
    assert_eq!(
        refusals.unknown,
        Some(FrameSceneTextureError::Unknown {
            handle: 5,
            resident: 1
        })
    );
}
