//! A game moves blocks a kernel would encode into a level of a declared block-compressed array,
//! and reads levels of its declared textures and targets back by handle after the frame, naming
//! no GPU type for either.
use super::common::{self, graphics};
use super::fixture::{Rig, SIZE, quad_scene};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{
    BlockSource, GraphicsProgram, ImportedDraw, ImportedGeometry, PreparedProgramDraw, TargetImage,
};
use worldrunner_render_program::state::RasterState;
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, GameBlockDestination, GameBuild, GameFeatureFactory, GameLevel, GamePassSpec,
        GamePasses, GameReadback, GameResourceError, GameResources, GameTargets, GameTexture,
        GameTextureDeclaration, GameTextureLevel, PassTarget, TargetDeclaration, TargetExtent,
    },
    executor::{
        AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
        GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
        ResourceRef,
    },
    wgpu,
};

/// The float target: two layers of 8 x 4 texels. The pass writes layer 1.
const FLOAT: &str = "work.float";
const FLOAT_SIZE: [u32; 2] = [8, 4];
const WRITTEN: ResourceRef = ResourceRef {
    name: FLOAT,
    version: 0,
    mip: 0,
    layer: 1,
};

const FLOAT_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let texel = floor(at.xy);
    return vec4<f32>(texel.x * 0.5 + texel.y * 64.0 + 0.0625, 0.0, 0.0, 1.0);
}";

/// What the float pass writes at a texel, exact in `f32`.
#[expect(clippy::cast_precision_loss, reason = "a handful of texels")]
fn float_at(x: u32, y: u32) -> f32 {
    x as f32 * 0.5 + y as f32 * 64.0 + 0.0625
}

/// The block a compressor kernel would have written: one `Rgba32Uint` texel holding one BC5
/// block, red then green.
const MOVED: [u8; 16] = [
    200, 17, 0x92, 0x24, 0x49, 0x92, 0x24, 0x49, 31, 240, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24,
];

/// What a game asks of its resources when it is built.
#[derive(Clone, Copy, Debug)]
enum Ask {
    /// A transfer into layer 1, mip 2 of a BC5 page array, and readbacks of that level and its
    /// neighbours.
    Transfer,
    /// Layer 1 of the float target, which the pass writes.
    Float,
    /// Mip 3 of both layers of an uploaded BC4 array.
    Bc4,
    /// Layer 1, mip 1 of an uploaded texture of each format the diagnostics dump.
    Formats,
}

/// A readback asked, with the level bytes it must deliver (`None` for the float target).
type Expected = (GameReadback, &'static str, [u32; 2], Option<Vec<u8>>);

#[derive(Default)]
struct Kept {
    resources: Option<GameResources>,
    reads: Vec<Expected>,
}

thread_local! {
    static KEPT: RefCell<Kept> = RefCell::new(Kept::default());
}

/// Empties [`KEPT`] when dropped, also while a failed assertion unwinds, so the renderer's
/// resources it holds drop before the thread's other locals.
struct ClearKept;

impl Drop for ClearKept {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(|kept| *kept = Kept::default());
    }
}

/// Bytes no two levels share, as an upload would carry.
fn pattern(len: u64, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from((index * 7 + u64::from(seed) * 31 + 3) % 251).unwrap())
        .collect()
}

/// A declared texture and the texels it was uploaded with.
struct Uploaded {
    texture: GameTexture,
    levels: Vec<GameTextureLevel>,
    texels: Vec<u8>,
}

impl Uploaded {
    /// The texels of level `mip` of `layer`, as the declaration lays them out.
    fn level(&self, layer: u32, mip: u32) -> Vec<u8> {
        let found = self
            .levels
            .iter()
            .find(|level| level.layer == layer && level.mip == mip)
            .unwrap();
        let start = usize::try_from(found.offset).unwrap();
        self.texels[start..start + usize::try_from(found.len).unwrap()].to_vec()
    }
}

/// Declares a two-layer array of `format` with uploaded texels no two levels share.
fn declare_uploaded(
    resources: &GameResources,
    label: &'static str,
    format: wgpu::TextureFormat,
    (side, mip_levels): ([u32; 2], u32),
) -> Result<Uploaded, GameResourceError> {
    let probe = GameTextureDeclaration {
        label,
        format,
        size: [side[0], side[1], 2],
        dimension: wgpu::TextureViewDimension::D2Array,
        mip_levels,
        texels: None,
    };
    let levels = probe.levels()?;
    let total = levels.iter().map(|level| level.len).sum();
    let seed = label.bytes().fold(0_u8, u8::wrapping_add);
    let texels = pattern(total, seed);
    let texture = resources.declare_texture(&GameTextureDeclaration {
        texels: Some(&texels),
        ..probe
    })?;
    Ok(Uploaded {
        texture,
        levels,
        texels,
    })
}

fn ask_transfer(resources: &GameResources) -> Result<Vec<Expected>, GameResourceError> {
    let format = wgpu::TextureFormat::Bc5RgUnorm;
    let uploaded = declare_uploaded(resources, "work.pages", format, ([16, 16], 4))?;
    let pages = uploaded.texture;
    let blocks = resources
        .runtime()
        .create_texture_2d(
            "work.blocks",
            [1, 1],
            wgpu::TextureFormat::Rgba32Uint,
            &MOVED,
        )
        .map_err(|error| GameResourceError::Declaration {
            label: "work.blocks".to_owned(),
            reason: error.to_string(),
        })?;
    resources.transfer_block_image(
        &BlockSource {
            texture: blocks,
            mip: 0,
            layer: 0,
            origin: [0, 0],
            extent: [1, 1],
        },
        GameBlockDestination {
            texture: pages,
            mip: 2,
            layer: 1,
            extent: [4, 4],
        },
    )?;
    let read = |layer, mip| resources.read_back_texture(pages, GameLevel { layer, mip });
    Ok(vec![
        (
            read(1, 2)?,
            "layer 1, mip 2 holds the moved block",
            [4, 4],
            Some(MOVED.to_vec()),
        ),
        (
            read(0, 2)?,
            "layer 0, mip 2 is as uploaded",
            [4, 4],
            Some(uploaded.level(0, 2)),
        ),
        (
            read(1, 1)?,
            "layer 1, mip 1 is as uploaded",
            [8, 8],
            Some(uploaded.level(1, 1)),
        ),
        (
            read(1, 3)?,
            "layer 1, mip 3 is as uploaded",
            [2, 2],
            Some(uploaded.level(1, 3)),
        ),
    ])
}

fn ask_bc4(resources: &GameResources) -> Result<Vec<Expected>, GameResourceError> {
    let format = wgpu::TextureFormat::Bc4RUnorm;
    let uploaded = declare_uploaded(resources, "work.heights", format, ([16, 16], 4))?;
    let heights = uploaded.texture;
    let read = |layer| resources.read_back_texture(heights, GameLevel { layer, mip: 3 });
    Ok(vec![
        (
            read(1)?,
            "BC4 layer 1, mip 3",
            [2, 2],
            Some(uploaded.level(1, 3)),
        ),
        (
            read(0)?,
            "BC4 layer 0, mip 3",
            [2, 2],
            Some(uploaded.level(0, 3)),
        ),
    ])
}

fn ask_formats(resources: &GameResources) -> Result<Vec<Expected>, GameResourceError> {
    let formats = [
        ("work.r32float", wgpu::TextureFormat::R32Float),
        ("work.rgba16float", wgpu::TextureFormat::Rgba16Float),
        ("work.rgba8unorm", wgpu::TextureFormat::Rgba8Unorm),
        ("work.r16uint", wgpu::TextureFormat::R16Uint),
        ("work.r8uint", wgpu::TextureFormat::R8Uint),
    ];
    formats
        .into_iter()
        .map(|(label, format)| {
            let uploaded = declare_uploaded(resources, label, format, ([8, 4], 2))?;
            let level = GameLevel { layer: 1, mip: 1 };
            let read = resources.read_back_texture(uploaded.texture, level)?;
            Ok((read, label, [4, 2], Some(uploaded.level(1, 1))))
        })
        .collect()
}

fn ask(
    ask: Ask,
    resources: &GameResources,
    targets: &GameTargets,
) -> Result<Vec<Expected>, GameResourceError> {
    match ask {
        Ask::Transfer => ask_transfer(resources),
        Ask::Float => Ok(vec![(
            resources.read_back_target(targets, FLOAT, GameLevel { layer: 1, mip: 0 })?,
            "layer 1 of the float target",
            FLOAT_SIZE,
            None,
        )]),
        Ask::Bc4 => ask_bc4(resources),
        Ask::Formats => ask_formats(resources),
    }
}

/// One pass before Opaque writing layer 1 of the float target.
struct Pass {
    targets: GameTargets,
    resources: GameResources,
    program: GraphicsProgram,
    draw: RefCell<Option<PreparedProgramDraw>>,
}

const ENTRY: PassEntry = PassEntry::nth(PassSlot::Game(GamePlace::BeforeOpaque), 0);

#[derive(Clone)]
struct PassFeature(Rc<Pass>);

impl FrameFeature for PassFeature {
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
            .writing([ColorOutput::Target(WRITTEN)]);
        Ok(FeatureDeclaration {
            passes: vec![self.0.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for PassFeature {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let pass = &self.0;
        if pass.draw.borrow().is_some() {
            return Ok(());
        }
        let target = PassTarget::of(context, ENTRY)?;
        let signature = target
            .signature()
            .map_err(|error| common::program_error("work", &error))?;
        let state = target.render_state(RasterState {
            cull: None,
            front_face: wgpu::FrontFace::Ccw,
            alpha_to_coverage: false,
        });
        let draw = pass
            .resources
            .runtime()
            .prepare_imported_draw(
                &ImportedDraw {
                    program: &pass.program,
                    state: &state,
                    vertex_inputs: &[],
                    bindings: &[],
                    imports: &[],
                    sets: &[],
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    geometry: &ImportedGeometry::Vertices { first: 0, count: 3 },
                    instances: 0..1,
                    owners: &[],
                },
                signature,
            )
            .map_err(|error| common::program_error("work", &error))?;
        target.admit("work", &draw)?;
        *pass.draw.borrow_mut() = Some(draw);
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, ENTRY);
        if let Some(draw) = self.0.draw.borrow().as_ref() {
            draw.record(pass);
        }
    }
}

struct WorkGame(Ask);

impl GameFeatureFactory for WorkGame {
    fn name(&self) -> &'static str {
        "transfer and readback"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            TargetDeclaration::new(
                FLOAT,
                wgpu::TextureFormat::R32Float,
                TargetExtent::Fixed(FLOAT_SIZE),
            )
            .layers(2)
            .with_usage(wgpu::TextureUsages::COPY_SRC),
        ]
    }

    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(1 << 20)
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        let resources = context.resources.clone();
        let reads = ask(self.0, &resources, &context.targets)?;
        let program = graphics(
            &resources.runtime(),
            FLOAT_WGSL,
            "vs",
            "fs",
            StageDeclarations::default(),
            StageDeclarations::default(),
        );
        KEPT.with_borrow_mut(|kept| {
            kept.resources = Some(resources.clone());
            kept.reads = reads;
        });
        Ok(Box::new(PassFeature(Rc::new(Pass {
            targets: context.targets.clone(),
            resources,
            program,
            draw: RefCell::new(None),
        }))))
    }
}

fn take(read: GameReadback) -> Result<Option<TargetImage>, GameResourceError> {
    KEPT.with_borrow(|kept| kept.resources.as_ref().unwrap().take_read_back(read))
}

fn poll(rig: &Rig) {
    rig.context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
}

/// Renders the game's first frame (after one cancelled frame with `cancel_first`) and returns
/// each readback's expectation with the image it delivered once the host polled the device.
fn frame(asked: Ask, cancel_first: bool) -> Vec<(Expected, TargetImage)> {
    let mut rig = Rig::with_features(
        &quad_scene(),
        SIZE,
        wgpu::Features::TEXTURE_COMPRESSION_BC,
        |settings| settings.game_passes = GamePasses::new().with(Arc::new(WorkGame(asked))),
    );
    let reads = KEPT.with_borrow(|kept| kept.reads.clone());
    if cancel_first {
        rig.render_cancelled().unwrap();
        poll(&rig);
        for (read, what, ..) in &reads {
            assert_eq!(
                take(*read).unwrap(),
                None,
                "{what}: a cancelled frame delivers nothing"
            );
        }
    }
    rig.render().unwrap();
    for (read, what, ..) in &reads {
        assert_eq!(
            take(*read).unwrap(),
            None,
            "{what} is delivered only after the host polls the device"
        );
    }
    poll(&rig);
    let delivered = reads
        .into_iter()
        .map(|expected| {
            let image = take(expected.0)
                .unwrap()
                .unwrap_or_else(|| panic!("{} is delivered after the frame", expected.1));
            assert!(
                matches!(take(expected.0), Err(GameResourceError::Readback { .. })),
                "{} is delivered once",
                expected.1
            );
            (expected, image)
        })
        .collect();
    drop(rig);
    delivered
}

/// Checks every delivered level against the bytes it must hold.
fn assert_levels(delivered: &[(Expected, TargetImage)], format: Option<wgpu::TextureFormat>) {
    for ((_, what, size, bytes), image) in delivered {
        assert_eq!(image.size, *size, "{what}: extent");
        if let Some(format) = format {
            assert_eq!(image.format, format, "{what}: format");
        }
        assert_eq!(Some(&image.bytes), bytes.as_ref(), "{what}: bytes");
    }
}

#[test]
fn a_block_transfer_fills_mip_two_of_a_declared_bc5_array_and_reads_back_exactly() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame(Ask::Transfer, false);
    assert_levels(&delivered, Some(wgpu::TextureFormat::Bc5RgUnorm));
}

#[test]
fn an_r32float_target_layer_one_reads_back_exactly() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame(Ask::Float, true);
    let [(_, image)] = delivered.as_slice() else {
        panic!("one readback")
    };
    assert_eq!(image.size, FLOAT_SIZE);
    assert_eq!(image.format, wgpu::TextureFormat::R32Float);
    let expected: Vec<u8> = (0..FLOAT_SIZE[1])
        .flat_map(|y| (0..FLOAT_SIZE[0]).map(move |x| float_at(x, y)))
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(image.bytes, expected, "layer 1 of the float target");
}

#[test]
fn a_bc4_mip_three_reads_back_the_uploaded_bytes() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame(Ask::Bc4, false);
    assert_levels(&delivered, Some(wgpu::TextureFormat::Bc4RUnorm));
}

#[test]
fn declared_textures_of_the_diagnostic_formats_read_back_exactly() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let delivered = frame(Ask::Formats, false);
    assert_levels(&delivered, None);
    let formats: Vec<_> = delivered.iter().map(|(_, image)| image.format).collect();
    assert_eq!(
        formats,
        [
            wgpu::TextureFormat::R32Float,
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::R16Uint,
            wgpu::TextureFormat::R8Uint,
        ]
    );
}
