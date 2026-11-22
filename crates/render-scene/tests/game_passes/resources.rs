//! A game's pass draws only with what the renderer owns: the buffers, textures and samplers its
//! factory declares to `GameBuild::resources`, written through the frame, and the scene's
//! resident textures, lent by handle. The game creates no GPU resource of its own.
use super::common::{self, texture_binding, uniform_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
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
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, ImportedDraw, ImportedGeometry, PreparedProgramDraw,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameSceneTextures, FrameSceneTexturesHandle, GameBuffer,
        GameBufferDeclaration, GameBufferUsage, GameBuild, GameFeatureFactory, GamePassSpec,
        GamePasses, GameResourceError, GameResources, GameSampler, GameTargets, GameTexture,
        GameTextureDeclaration, PassTarget, TargetDeclaration, TargetExtent,
    },
    draw::PreparedTextureReplacement,
    wgpu,
};

const BUDGET: u64 = 1 << 16;
/// The green the declared texture holds.
const DECLARED_GREEN: u8 = 200;
/// The scene texture's blue before and after the test replaces it (the stock texel is
/// `[200, 120, 60, 255]`).
const SCENE_BLUE: u8 = 60;
const REPLACED_BLUE: u8 = 250;

const WGSL: &str = "
struct Tint { value: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@group(0) @binding(1) var declared: texture_2d<f32>;
@group(0) @binding(2) var scene: texture_2d<f32>;
@group(0) @binding(3) var point: sampler;
@vertex fn vs(@location(0) corner: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(corner, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    let at = vec2<f32>(0.5, 0.5);
    return vec4<f32>(
        tint.value.x,
        textureSample(declared, point, at).g,
        textureSample(scene, point, at).b,
        1.0,
    );
}";

/// What one game's prepared frames saw that is not `Send`.
#[derive(Default)]
struct Kept {
    resources: Option<GameResources>,
    tint: Option<GameBuffer>,
    scene: Option<FrameSceneTexturesHandle>,
    snapshots: Vec<FrameSceneTextures>,
}

thread_local! {
    /// Per game (by its target), what it kept on the test's thread.
    static KEPT: RefCell<Vec<(&'static str, Kept)>> = const { RefCell::new(Vec::new()) };
}

/// Empties [`KEPT`] when dropped, also while a failed assertion unwinds, so the renderer's
/// resources it holds drop before the thread's other locals are destroyed.
struct ClearKept;

impl Drop for ClearKept {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(Vec::clear);
    }
}

fn kept<T>(game: &'static str, read: impl FnOnce(&mut Kept) -> T) -> T {
    KEPT.with_borrow_mut(|all| {
        if !all.iter().any(|(name, _)| *name == game) {
            all.push((game, Kept::default()));
        }
        let (_, kept) = all.iter_mut().find(|(name, _)| *name == game).unwrap();
        read(kept)
    })
}

/// The refusals the game met, by text.
type Seen = Arc<Mutex<Vec<String>>>;

/// What the test asks a game's next frames to do.
#[derive(Default)]
struct Controls {
    /// Leave the tint as the last frame wrote it.
    skip_write: AtomicBool,
    /// Refuse the frame after writing the tint.
    fail_after_write: AtomicBool,
    /// Try writes outside the tint and report how each was refused.
    probe_writes: AtomicBool,
}

struct ResourceGame {
    /// The target it draws into, which also names the game.
    out: &'static str,
    /// The occurrence of its pass before Opaque.
    occurrence: u32,
    seen: Seen,
    controls: Arc<Controls>,
}

struct Handles {
    vertices: GameBuffer,
    tint: GameBuffer,
    declared: GameTexture,
    point: GameSampler,
}

fn semantic(name: &str) -> EngineSemantic {
    EngineSemantic::named(name)
}

fn declare(resources: &GameResources) -> Result<Handles, GameResourceError> {
    let corners: Vec<u8> = [-1.0_f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    Ok(Handles {
        vertices: resources.declare_buffer(&GameBufferDeclaration {
            label: "resources.vertices",
            usage: GameBufferUsage::Vertex,
            contents: &corners,
        })?,
        tint: resources.declare_buffer(&GameBufferDeclaration {
            label: "resources.tint",
            usage: GameBufferUsage::Uniform,
            contents: &[0; 16],
        })?,
        declared: resources.declare_texture(&GameTextureDeclaration {
            label: "resources.declared",
            format: wgpu::TextureFormat::Rgba8Unorm,
            size: [1, 1, 1],
            dimension: wgpu::TextureViewDimension::D2,
            mip_levels: 1,
            texels: Some(&[0, DECLARED_GREEN, 0, 255]),
        })?,
        point: resources.declare_sampler(&wgpu::SamplerDescriptor::default()),
    })
}

fn program(resources: &GameResources) -> GraphicsProgram {
    let filterable = wgpu::TextureSampleType::Float { filterable: true };
    let fragment = StageDeclarations {
        resources: ResourceLayout::new(vec![
            uniform_binding(0, "resources.tint"),
            texture_binding(1, "resources.declared", filterable),
            texture_binding(2, "resources.scene", filterable),
            ResourceBinding {
                at: BindingAddress { set: 0, binding: 3 },
                source: BindingSource::Engine(semantic("resources.point")),
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

impl GameFeatureFactory for ResourceGame {
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
        AllocationBudget(BUDGET)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let resources = context.resources.clone();
        let handles = declare(&resources)?;
        kept(self.out, |kept| {
            kept.resources = Some(resources.clone());
            kept.tint = Some(handles.tint);
            kept.scene = Some(context.scene_textures.clone());
        });
        Ok(Box::new(Game(Rc::new(Parts {
            out: self.out,
            entry: PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), self.occurrence),
            targets: context.targets.clone(),
            program: program(&resources),
            resources,
            scene: context.scene_textures.clone(),
            handles,
            seen: self.seen.clone(),
            controls: self.controls.clone(),
            frames: RefCell::new(0),
            draw: RefCell::new(None),
        }))))
    }
}

struct Parts {
    out: &'static str,
    entry: PassEntry,
    targets: GameTargets,
    resources: GameResources,
    scene: FrameSceneTexturesHandle,
    handles: Handles,
    program: GraphicsProgram,
    seen: Seen,
    controls: Arc<Controls>,
    frames: RefCell<u32>,
    /// The draw, and the scene texture snapshot it binds.
    draw: RefCell<Option<(FrameSceneTextures, PreparedProgramDraw)>>,
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

impl Game {
    fn prepare_draw(
        &self,
        context: &PrepareContext<'_>,
        textures: &FrameSceneTextures,
    ) -> Result<PreparedProgramDraw, FrameError> {
        let parts = &self.0;
        let resources = &parts.resources;
        let handles = &parts.handles;
        let imports = [
            resources.import_buffer(semantic("resources.tint"), handles.tint)?,
            resources.import_texture(semantic("resources.declared"), handles.declared)?,
            textures.import(semantic("resources.scene"), TextureHandle::from_raw(0))?,
            resources.import_sampler(semantic("resources.point"), handles.point)?,
        ];
        let vertices = resources.vertex_input(
            handles.vertices,
            (8, wgpu::VertexStepMode::Vertex),
            wgpu::vertex_attr_array![0 => Float32x2].to_vec(),
        )?;
        let target = PassTarget::of(context, parts.entry)?;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("resource game", &error))?;
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
            .map_err(|error| common::program_error("resource game", &error))?;
        target.admit("resource game", &draw)?;
        Ok(draw)
    }
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let parts = &self.0;
        let frame = {
            let mut frames = parts.frames.borrow_mut();
            *frames += 1;
            *frames
        };
        // Frame n writes n / 4 into the red the pass outputs.
        #[expect(clippy::cast_precision_loss, reason = "a handful of frames")]
        let red = frame as f32 / 4.0;
        let bytes: Vec<u8> = [red, 0.0, 0.0, 0.0]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        if !parts.controls.skip_write.load(Ordering::Relaxed) {
            parts
                .resources
                .write(context, parts.handles.tint, 0, &bytes)?;
        }
        if parts.controls.probe_writes.load(Ordering::Relaxed) {
            // The tint is 16 bytes: too long, too far, and an offset that wraps.
            for (offset, len) in [(0, 20), (12, 8), (u64::MAX - 3, 4)] {
                let refused =
                    parts
                        .resources
                        .write(context, parts.handles.tint, offset, &vec![0; len]);
                parts.seen.lock().unwrap().push(match refused {
                    Err(refused) => refused.to_string(),
                    Ok(()) => format!("accepted {len} bytes at {offset}"),
                });
            }
        }
        if parts.controls.fail_after_write.load(Ordering::Relaxed) {
            return Err(FrameError::FeaturePreparation {
                feature: "resource game",
                reason: "refused on purpose".to_owned(),
            });
        }
        if frame == 1
            && let Err(refused) = parts
                .resources
                .write(context, parts.handles.tint, 2, &[0; 4])
        {
            parts.seen.lock().unwrap().push(refused.to_string());
        }
        let textures = parts.scene.current()?;
        kept(parts.out, |kept| kept.snapshots.push(textures.clone()));
        let current = parts
            .draw
            .borrow()
            .as_ref()
            .is_some_and(|(made, _)| made.same_snapshot(&textures));
        if !current {
            let draw = self.prepare_draw(context, &textures)?;
            *parts.draw.borrow_mut() = Some((textures, draw));
        }
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, self.0.entry);
        if let Some((_, draw)) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

/// `code` of an sRGB texel as the linear value a sampler returns, stored back in 8 bits.
fn linear(code: u8) -> u8 {
    let value = f64::from(code) / 255.0;
    let linear = if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a value in 0..=255"
    )]
    let stored = (linear * 255.0).round() as u8;
    stored
}

fn output(rig: &Rig, out: &'static str) -> [u8; 4] {
    let texture = rig.renderer.game_targets().texture(out).unwrap();
    Rig::pixel(&rig.read(&texture), 5, 7)
}

fn near(found: [u8; 4], expected: [u8; 4]) -> bool {
    found
        .iter()
        .zip(expected)
        .all(|(found, expected)| found.abs_diff(expected) <= 1)
}

/// What the games kept: one snapshot per table, the declared bytes, and the refusals of a
/// misused, oversized, empty or foreign resource.
fn check_stores() {
    kept("resources.first", |kept| {
        let snapshots = &kept.snapshots;
        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots[0].len(), 1);
        assert!(
            snapshots[0].same_snapshot(&snapshots[1]),
            "one snapshot while the table holds"
        );
        assert!(
            !snapshots[1].same_snapshot(&snapshots[2]),
            "a replacement makes a new one"
        );
        let resources = kept.resources.as_ref().unwrap();
        assert_eq!(
            resources.bytes(),
            24 + 16 + 4,
            "vertices, tint and one texel"
        );
        let tint = kept.tint.unwrap();
        assert!(matches!(
            resources.vertex_input(tint, (16, wgpu::VertexStepMode::Vertex), Vec::new()),
            Err(GameResourceError::Usage { .. })
        ));
        assert!(matches!(
            resources.declare_buffer(&GameBufferDeclaration {
                label: "resources.too-big",
                usage: GameBufferUsage::Storage,
                contents: &vec![0; usize::try_from(BUDGET).unwrap()],
            }),
            Err(GameResourceError::Budget { .. })
        ));
        assert!(matches!(
            resources.declare_buffer(&GameBufferDeclaration {
                label: "resources.empty",
                usage: GameBufferUsage::Uniform,
                contents: &[],
            }),
            Err(GameResourceError::Declaration { .. })
        ));
    });
    let foreign = kept("resources.second", |kept| kept.tint.unwrap());
    kept("resources.first", |kept| {
        assert_eq!(
            kept.resources
                .as_ref()
                .unwrap()
                .import_buffer(semantic("resources.tint"), foreign),
            Err(GameResourceError::Foreign("game buffer"))
        );
    });
}

#[test]
fn a_game_pass_draws_with_declared_resources_and_lent_scene_textures() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    KEPT.with_borrow_mut(Vec::clear);
    let _clear = ClearKept;
    let seen = Seen::default();
    let game = |out, occurrence| {
        Arc::new(ResourceGame {
            out,
            occurrence,
            seen: seen.clone(),
            controls: Arc::default(),
        })
    };
    let scene = quad_scene();
    let mut rig = Rig::new(&scene, SIZE, |settings| {
        settings.game_passes = GamePasses::new()
            .with(game("resources.first", 0))
            .with(game("resources.second", 1));
    });

    rig.render().unwrap();
    let first = output(&rig, "resources.first");
    assert!(
        near(first, [64, DECLARED_GREEN, linear(SCENE_BLUE), 255]),
        "frame 1 drew the written tint, the declared texel and the lent scene texel: {first:?}"
    );
    rig.render().unwrap();
    let second = output(&rig, "resources.first");
    assert!(
        near(second, [128, DECLARED_GREEN, linear(SCENE_BLUE), 255]),
        "frame 2's write reached the pass: {second:?}"
    );
    assert_eq!(
        output(&rig, "resources.second"),
        second,
        "each factory draws with its own store"
    );

    // The renderer replaces the scene texture; the next frame lends a new snapshot.
    let replacement = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![10, 20, REPLACED_BLUE, 255].into(),
    };
    let prepared =
        PreparedTextureReplacement::new(&scene, TextureHandle::from_raw(0), &replacement).unwrap();
    rig.gpu
        .replace_texture_prepared(
            rig.context.device(),
            rig.context.submission(),
            &rig.pipelines,
            &prepared,
        )
        .unwrap();
    rig.render().unwrap();
    let third = output(&rig, "resources.first");
    assert!(
        near(third, [191, DECLARED_GREEN, linear(REPLACED_BLUE), 255]),
        "frame 3 bound the replaced scene texture: {third:?}"
    );

    check_stores();
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [
            "write of 4 bytes at 2 into game buffer resources.tint of 16 bytes",
            "write of 4 bytes at 2 into game buffer resources.tint of 16 bytes",
        ],
        "each game's misaligned write was refused by name"
    );
}

#[path = "resources/contracts.rs"]
mod contracts;
