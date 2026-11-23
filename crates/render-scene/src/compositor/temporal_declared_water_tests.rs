//! What a renderer whose game draws the water reports and accepts at the two renderer-level entries
//! that used to treat any installed water as drawn. The capture contract lists the writes the
//! frame's passes make, so water that no pass draws adds no snapshot or receiver writes unless
//! something else reads the snapshot banks. The geometry-only `encode` refuses water that the
//! stock passes draw and accepts water that they do not, as it accepts a scene without water.
use super::taa_end_to_end_tests::{OUTPUT, context};
use super::temporal_water_tests::{body, frame, scene_bytes};
use crate::compositor::{
    ConfiguredFrameRenderer, GameBuild, GameFeatureFactory, GamePasses, RendererConfiguration,
    SnapshotReads, StockReplacement, attachment, declaring_stock,
};
use crate::device::GpuContext;
use std::sync::Arc;
use worldrunner_render_gpu::{
    configuration::AllocationBudget, submission::PrepareError, submission::capture::WriteKey,
};
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, BorrowedFeature, BuildContext, DrawContext, FeatureDeclaration,
    FrameConfig, FrameError, FrameFeature, PassDeclaration, PassSlot, PrepareContext,
    PreparedFeature, ResourceRef, ToneMap,
};

const EXTENT: [u32; 2] = [64, 64];
const REFUSAL: &str = "SSAO requires stock geometry composition";

/// An HDR renderer with, when `water`, one body, and the game inventory `game`.
fn settings(water: bool, game: GamePasses) -> RendererConfiguration {
    let mut settings = RendererConfiguration::new(EXTENT, OUTPUT, ToneMap::Reinhard);
    settings.frame = FrameConfig::new(EXTENT, wgpu::TextureFormat::Rgba16Float, 1);
    settings.game_passes = game;
    if water {
        settings.water = vec![body(false)];
    }
    settings
}

fn build(context: &GpuContext, settings: RendererConfiguration) -> ConfiguredFrameRenderer {
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap()
}

/// The kinds of `keys` in order. Resource and publication keys carry the identity of the
/// renderer that made them, so two renderers report the same writes as the same sequence of
/// kinds.
fn kinds(keys: &[WriteKey]) -> Vec<&'static str> {
    keys.iter()
        .map(|key| match key {
            WriteKey::Resident(_) => "resident",
            WriteKey::Readback(_) => "readback",
            WriteKey::Resource { .. } => "resource",
        })
        .collect()
}

/// The effect keys and the write keys of a capture of this renderer.
fn keys(
    context: &GpuContext,
    renderer: &mut ConfiguredFrameRenderer,
) -> (Vec<WriteKey>, Vec<WriteKey>) {
    let frame = renderer.renderer_mut();
    let effects = frame.capture_effect_keys(context.device()).unwrap();
    (effects, frame.capture_write_keys())
}

#[test]
fn gpu_declared_water_reports_the_capture_keys_of_a_renderer_without_water() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (dry_effects, dry_writes) = keys(
        &context,
        &mut build(&context, settings(false, GamePasses::new())),
    );
    let (hidden_effects, hidden_writes) = keys(
        &context,
        &mut build(
            &context,
            settings(true, declaring_stock(StockReplacement::WATER)),
        ),
    );
    let (drawn_effects, drawn_writes) = keys(
        &context,
        &mut build(&context, settings(true, GamePasses::new())),
    );

    assert_eq!(kinds(&hidden_effects), kinds(&dry_effects), "effect keys");
    assert_eq!(kinds(&hidden_writes), kinds(&dry_writes), "write keys");
    // Stock water adds the three opaque snapshot banks and the three water receivers.
    assert_eq!(
        drawn_effects.len(),
        dry_effects.len() + 6,
        "stock effect keys"
    );
    assert_eq!(kinds(&drawn_writes), kinds(&dry_writes), "stock write keys");
}

/// A game feature that reads the opaque colour snapshot and draws nothing, and draws `replaces`.
struct Reader {
    replaces: StockReplacement,
}

struct ReadsSnapshot;

struct Idle;

impl GameFeatureFactory for Reader {
    fn name(&self) -> &'static str {
        "snapshot reader"
    }

    fn snapshot_reads(&self) -> SnapshotReads {
        SnapshotReads {
            color: true,
            ..SnapshotReads::NONE
        }
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.replaces
    }

    fn build(&self, _: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(ReadsSnapshot))
    }
}

impl FrameFeature for ReadsSnapshot {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::ScreenSpaceAfterTransparent);
        declaration.passes[0].reads = vec![ResourceRef::new("opaque snapshot color")];
        declaration
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Idle))
    }
}

impl PreparedFeature for Idle {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

#[test]
fn gpu_declared_water_still_lists_the_snapshot_banks_a_game_feature_reads() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let reading = |water: bool, replaces: StockReplacement| {
        settings(water, GamePasses::new().with(Arc::new(Reader { replaces })))
    };
    let (plain, _) = keys(
        &context,
        &mut build(&context, settings(false, GamePasses::new())),
    );
    let (dry, _) = keys(
        &context,
        &mut build(&context, reading(false, StockReplacement::NONE)),
    );
    let (hidden, _) = keys(
        &context,
        &mut build(&context, reading(true, StockReplacement::WATER)),
    );

    // The reader makes the frame write the colour, depth and final-depth banks.
    assert_eq!(dry.len(), plain.len() + 3, "a reader without water");
    assert_eq!(kinds(&hidden), kinds(&dry), "a reader with declared water");
}

/// The declaration of a caller feature that reads the opaque colour snapshot after transparents.
fn reads_snapshot() -> FeatureDeclaration {
    ReadsSnapshot.declare()
}

#[test]
fn gpu_declared_water_lists_the_snapshot_banks_once_a_caller_feature_has_read_them() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let mut renderer = build(
        &context,
        settings(true, declaring_stock(StockReplacement::WATER)),
    );
    let (before, _) = keys(&context, &mut renderer);
    let (opaque, views) = opaque(&renderer);
    let mut output = opaque.clone();
    output.slot = PassSlot::ScreenSpaceAfterTransparent;
    for attachment in output.color.iter_mut().chain(&mut output.depth) {
        attachment.load = AttachmentLoad::Load;
    }
    let mut idle = Idle;
    let uniform = frame(EXTENT[0]);
    let mut encoder = context.submission().begin(device).unwrap();
    renderer
        .renderer_mut()
        .encode_frame(
            device,
            &mut encoder,
            (&[opaque, output], &views),
            None,
            &mut [BorrowedFeature {
                declaration: reads_snapshot(),
                prepared: &mut idle,
            }],
            &(uniform, None),
        )
        .unwrap();
    encoder.finish().submit().unwrap();

    let (after, _) = keys(&context, &mut renderer);
    assert_eq!(after.len(), before.len() + 3, "the banks the frame wrote");
}

/// The scene colour and depth of the renderer as one geometry-only opaque pass that clears the
/// colour to a visible grey.
fn opaque(renderer: &ConfiguredFrameRenderer) -> (PassDeclaration, [AttachmentView; 2]) {
    let targets = renderer.targets();
    let color =
        AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture()).unwrap();
    let depth =
        AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture()).unwrap();
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(color.resource(), &color, false));
    pass.depth = Some(attachment(depth.resource(), &depth, true));
    if let Some(color) = &mut pass.color {
        color.load = AttachmentLoad::ClearColor(wgpu::Color {
            r: 0.25,
            g: 0.5,
            b: 0.75,
            a: 1.0,
        });
    }
    (pass, [color, depth])
}

/// One geometry-only frame through `FrameRenderer::encode`, and the scene colour it left.
fn encode(
    context: &GpuContext,
    renderer: &mut ConfiguredFrameRenderer,
) -> Result<Vec<u8>, FrameError> {
    let device = context.device();
    let (pass, views) = opaque(renderer);
    let frame = renderer.renderer_mut();
    let transaction = context
        .submission()
        .prepare(device, |encoder, _| {
            frame.encode(device, encoder, &[pass], &views, &mut [])
        })
        .map_err(|error| match error {
            PrepareError::Preparation(error) => error,
            other => panic!("the transaction did not start: {other:?}"),
        })?;
    transaction.submit().unwrap();
    Ok(scene_bytes(context, renderer.renderer_mut()))
}

#[test]
fn gpu_declared_water_encodes_geometry_only_frames_like_a_scene_without_water() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let control = encode(
        &context,
        &mut build(&context, settings(false, GamePasses::new())),
    )
    .expect("a scene without water encodes");
    let mut hidden = build(
        &context,
        settings(true, declaring_stock(StockReplacement::WATER)),
    );
    for frame in 0..2 {
        let output = encode(&context, &mut hidden)
            .unwrap_or_else(|error| panic!("frame {frame} refused: {error}"));
        assert!(
            output == control,
            "frame {frame} is the frame without water"
        );
    }
}

#[test]
fn gpu_stock_water_still_refuses_geometry_only_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut stock = build(&context, settings(true, GamePasses::new()));
    let error = encode(&context, &mut stock).expect_err("stock water needs stock composition");
    assert_eq!(error, FrameError::UnsupportedEffect(REFUSAL));
}
