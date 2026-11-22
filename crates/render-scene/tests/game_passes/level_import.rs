//! A game binds one level of one layer of a declared array or target by handle, sampled by a
//! pass or written by a kernel, without making a view of its own, and only that level is read
//! or written.
use super::common::{self, texture_binding};
use super::fixture::{Rig, SIZE, quad_scene};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_program::binding::{EngineSemantic, ResourceLayout};
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    GraphicsProgram, Import, ImportedDraw, ImportedGeometry, ImportedResource, PreparedProgramDraw,
    TargetImage,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, GameBuild, GameFeatureFactory, GameLevel, GamePassSpec, GamePasses,
        GameReadback, GameResourceError, GameResources, GameTargets, GameTexture,
        GameTextureDeclaration, LevelAccess, PassTarget, TargetDeclaration, TargetExtent,
    },
    wgpu,
};

/// The four-layer target the kernel writes layer 2 of, through a one-layer import.
const LAYERS: &str = "level.layers";
const LAYERS_SIZE: [u32; 2] = [8, 4];
const WRITTEN_LAYER: u32 = 2;
/// The target the pass writes what it loads from layer 1, mip 1 of the declared array.
const OUT: &str = "level.out";
/// The declared array: two layers of 8 x 4, 4 x 2 and 2 x 1 `Rgba8Unorm` texels.
const SOURCE: &str = "level.source";
const SAMPLED: GameLevel = GameLevel { layer: 1, mip: 1 };
const SAMPLED_SIZE: [u32; 2] = [4, 2];

/// The pass loads level 0 of what it is lent, which is layer 1, mip 1 of the array.
const PASS_WGSL: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(at.xy), 0);
}";

/// The pass of a game that reads what the kernel wrote: level 0 of the one-layer import of the
/// target's layer, scaled to a colour.
const FROM_KERNEL_WGSL: &str = "
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(source, vec2<i32>(at.xy), 0).x / 1100.0, 0.0, 0.0, 1.0);
}";

/// The kernel writes every texel of the 2D image it is lent.
const KERNEL_WGSL: &str = "
@group(0) @binding(0) var written: texture_storage_2d<r32float, write>;
@compute @workgroup_size(8, 4) fn kernel(@builtin(global_invocation_id) id: vec3<u32>) {
    let value = f32(id.x) * 0.5 + f32(id.y) * 64.0 + 1000.0625;
    textureStore(written, vec2<i32>(id.xy), vec4<f32>(value, 0.0, 0.0, 1.0));
}";

/// What the kernel writes at a texel, exact in `f32`.
#[expect(clippy::cast_precision_loss, reason = "a handful of texels")]
fn written_at(x: u32, y: u32) -> f32 {
    x as f32 * 0.5 + y as f32 * 64.0 + 1000.0625
}

/// Bytes no two levels share, as an upload would carry.
fn pattern(len: u64) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from((index * 7 + 3) % 251).unwrap())
        .collect()
}

/// What the game asks for when it is built: the readbacks, by what each must hold.
struct Asked {
    resources: GameResources,
    layers: Vec<(u32, GameReadback)>,
    out: GameReadback,
    /// The texels of layer 1, mip 1 of the declared array, as uploaded.
    sampled: Vec<u8>,
}

thread_local! {
    static KEPT: RefCell<Option<Asked>> = const { RefCell::new(None) };
}

/// Empties [`KEPT`] when dropped, also while a failed assertion unwinds, so the renderer's
/// resources it holds drop before the thread's other locals.
struct ClearKept;

impl Drop for ClearKept {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(|kept| *kept = None);
    }
}

/// Declares the two-layer, three-mip array with texels no two levels share, and returns it with
/// the texels of [`SAMPLED`].
fn declare_source(resources: &GameResources) -> Result<(GameTexture, Vec<u8>), GameResourceError> {
    let probe = GameTextureDeclaration {
        label: SOURCE,
        format: wgpu::TextureFormat::Rgba8Unorm,
        size: [LAYERS_SIZE[0], LAYERS_SIZE[1], 2],
        dimension: wgpu::TextureViewDimension::D2Array,
        mip_levels: 3,
        texels: None,
    };
    let levels = probe.levels()?;
    let texels = pattern(levels.iter().map(|level| level.len).sum());
    let found = levels
        .iter()
        .find(|level| level.layer == SAMPLED.layer && level.mip == SAMPLED.mip)
        .unwrap();
    let start = usize::try_from(found.offset).unwrap();
    let sampled = texels[start..start + usize::try_from(found.len).unwrap()].to_vec();
    let texture = resources.declare_texture(&GameTextureDeclaration {
        texels: Some(&texels),
        ..probe
    })?;
    Ok((texture, sampled))
}

const ENTRY: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);

/// The pass that loads the sampled level into [`OUT`], and the kernel when the game writes.
struct Parts {
    targets: GameTargets,
    resources: GameResources,
    source: GameTexture,
    program: GraphicsProgram,
    kernel: bool,
    /// The pass loads the layer the kernel wrote, instead of a level of the declared array.
    from_kernel: bool,
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
        let spec = GamePassSpec::new(GamePlace::BeforeOpaque, ENTRY.occurrence())
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

/// Stages the kernel over the one-layer import of layer [`WRITTEN_LAYER`] of [`LAYERS`]. The
/// test builds the kernel's pipeline and group itself from the import's view; what is under test
/// is that the view is exactly that layer.
fn stage_kernel(context: &mut PrepareContext<'_>, import: &Import) {
    let ImportedResource::Texture(lent) = &import.resource else {
        panic!("a texture is lent, got {:?}", import.resource)
    };
    assert_eq!(lent.dimension, wgpu::TextureViewDimension::D2);
    let device = context.device().clone();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("level import kernel"),
        source: wgpu::ShaderSource::Wgsl(KERNEL_WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("level import kernel"),
        layout: None,
        module: &module,
        entry_point: Some("kernel"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("level import kernel"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&lent.view),
        }],
    });
    context.dispatch("level import kernel", &pipeline, &group, [1, 1, 1]);
}

impl PreparedFeature for Game {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let parts = &self.0;
        if parts.draw.borrow().is_some() {
            return Ok(());
        }
        if parts.kernel {
            let import = parts.targets.import_level(
                LAYERS,
                EngineSemantic::named("level.written"),
                GameLevel {
                    layer: WRITTEN_LAYER,
                    mip: 0,
                },
                LevelAccess::Storage,
            )?;
            stage_kernel(context, &import);
        }
        let sampled = if parts.from_kernel {
            parts.targets.import_level(
                LAYERS,
                EngineSemantic::named(SOURCE),
                GameLevel {
                    layer: WRITTEN_LAYER,
                    mip: 0,
                },
                LevelAccess::Sampled,
            )?
        } else {
            parts.resources.import_texture_level(
                EngineSemantic::named(SOURCE),
                parts.source,
                SAMPLED,
                LevelAccess::Sampled,
            )?
        };
        let target = PassTarget::of(context, ENTRY)?;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("level", &error))?;
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
                    imports: &[sampled],
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                signature,
            )
            .map_err(|error| common::program_error("level", &error))?;
        target.admit("level", &draw)?;
        *parts.draw.borrow_mut() = Some(draw);
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, ENTRY);
        if let Some(draw) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

/// A game whose pass samples one level of its declared array, and whose kernel, when `kernel`,
/// writes one layer of its four-layer target.
struct LevelGame {
    kernel: bool,
    from_kernel: bool,
}

impl GameFeatureFactory for LevelGame {
    fn name(&self) -> &'static str {
        "level import"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            TargetDeclaration::new(
                LAYERS,
                wgpu::TextureFormat::R32Float,
                TargetExtent::Fixed(LAYERS_SIZE),
            )
            .layers(4)
            .with_usage(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC),
            TargetDeclaration::new(
                OUT,
                wgpu::TextureFormat::Rgba8Unorm,
                TargetExtent::Fixed(SAMPLED_SIZE),
            )
            .with_usage(wgpu::TextureUsages::COPY_SRC),
        ]
    }

    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(1 << 20)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let resources = context.resources.clone();
        let targets = &context.targets;
        let (source, sampled) = declare_source(&resources)?;
        let layers = (0..4)
            .map(|layer| {
                let level = GameLevel { layer, mip: 0 };
                Ok((layer, resources.read_back_target(targets, LAYERS, level)?))
            })
            .collect::<Result<Vec<_>, GameResourceError>>()?;
        let out = resources.read_back_target(targets, OUT, GameLevel::default())?;
        let (wgsl, filterable) = if self.from_kernel {
            (FROM_KERNEL_WGSL, false)
        } else {
            (PASS_WGSL, true)
        };
        let program = common::graphics(
            &resources.runtime(),
            wgsl,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations {
                resources: ResourceLayout::new(vec![texture_binding(
                    0,
                    SOURCE,
                    wgpu::TextureSampleType::Float { filterable },
                )]),
                constants: Vec::new(),
            },
        );
        KEPT.with_borrow_mut(|kept| {
            *kept = Some(Asked {
                resources: resources.clone(),
                layers,
                out,
                sampled,
            });
        });
        Ok(Box::new(Game(Rc::new(Parts {
            targets: targets.clone(),
            resources,
            source,
            program,
            kernel: self.kernel,
            from_kernel: self.from_kernel,
            draw: RefCell::new(None),
        }))))
    }
}

/// What one frame of the game delivered: each layer of [`LAYERS`] by layer, and [`OUT`].
struct Delivered {
    layers: Vec<(u32, TargetImage)>,
    out: TargetImage,
    sampled: Vec<u8>,
}

/// Renders the game's first frame and takes every readback: the first take starts its map, and
/// the level is delivered once the host has polled the device.
fn frame(kernel: bool) -> Delivered {
    frame_of(LevelGame {
        kernel,
        from_kernel: false,
    })
}

/// [`frame`] of `game`.
fn frame_of(game: LevelGame) -> Delivered {
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(Arc::new(game));
    });
    rig.render().unwrap();
    KEPT.with_borrow(|kept| {
        let asked = kept.as_ref().expect("the game was built");
        let reads = asked.layers.iter().map(|&(_, read)| read);
        for read in reads.chain([asked.out]) {
            assert_eq!(asked.resources.take_read_back(read).unwrap(), None);
        }
    });
    rig.context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    let delivered = KEPT.with_borrow(|kept| {
        let asked = kept.as_ref().expect("the game was built");
        let take = |read| {
            asked
                .resources
                .take_read_back(read)
                .unwrap()
                .expect("delivered after the frame")
        };
        Delivered {
            layers: asked
                .layers
                .iter()
                .map(|&(layer, read)| (layer, take(read)))
                .collect(),
            out: take(asked.out),
            sampled: asked.sampled.clone(),
        }
    });
    drop(rig);
    delivered
}

#[test]
fn a_kernel_writes_layer_two_of_a_four_layer_target_through_a_one_layer_import() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame(true);
    let [width, height] = LAYERS_SIZE;
    let written: Vec<u8> = (0..height)
        .flat_map(|y| (0..width).map(move |x| written_at(x, y)))
        .flat_map(f32::to_le_bytes)
        .collect();
    let untouched = vec![0_u8; written.len()];
    assert_eq!(delivered.layers.len(), 4);
    for (layer, image) in &delivered.layers {
        assert_eq!(image.size, LAYERS_SIZE, "layer {layer}: extent");
        assert_eq!(image.format, wgpu::TextureFormat::R32Float);
        let expected = if *layer == WRITTEN_LAYER {
            &written
        } else {
            &untouched
        };
        assert_eq!(&image.bytes, expected, "layer {layer}");
    }
}

#[test]
fn a_pass_loads_exactly_layer_one_mip_one_of_a_declared_array_through_a_one_level_import() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame(false);
    assert_eq!(delivered.out.size, SAMPLED_SIZE);
    assert_eq!(delivered.out.format, wgpu::TextureFormat::Rgba8Unorm);
    assert_eq!(
        delivered.out.bytes, delivered.sampled,
        "the pass loads layer 1, mip 1 as uploaded"
    );
    for (layer, image) in &delivered.layers {
        assert!(
            image.bytes.iter().all(|&byte| byte == 0),
            "layer {layer}: nothing writes the target without the kernel"
        );
    }
}

#[test]
fn a_pass_loads_the_layer_a_kernel_wrote_in_the_same_frame_through_a_one_layer_import() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame_of(LevelGame {
        kernel: true,
        from_kernel: true,
    });
    assert_eq!(delivered.out.size, SAMPLED_SIZE);
    for y in 0..SAMPLED_SIZE[1] {
        for x in 0..SAMPLED_SIZE[0] {
            let red = delivered.out.pixel(x, y)[0];
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a unit colour"
            )]
            let expected = (written_at(x, y) / 1100.0 * 255.0).round() as u8;
            assert!(
                red.abs_diff(expected) <= 1,
                "texel ({x}, {y}): the pass read {red}, the kernel wrote {expected}"
            );
        }
    }
}
