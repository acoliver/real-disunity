//! When the stock frame plans and writes the post-water depth, and what the pass that writes it
//! carries. The stock water, the stock reflections and a game pass that declares a read each ask
//! for it; water that a game draws, which no stock pass draws, asks for nothing, so a frame plans
//! the pass only when something else reads it. The device depth companion is a second colour
//! attachment of that pass, which only a game that reads it adds.
use super::water_draws::{Frame, rig_configured};
use super::*;
use crate::compositor::{
    ColorOutput, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses, GameTargets,
    PostWaterDepth, SnapshotReads, StockReplacement, declaring_stock,
};
use std::sync::Arc;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedFeature,
};

const LINEAR: &str = "AfterWaterDepth#0 post-water depth:C / ";
const WITH_DEVICE: &str = "AfterWaterDepth#0 post-water depth:C,post-water device depth:C / ";
const OPAQUE_DEPTHS: &str =
    "AfterOpaqueDepth#0 opaque snapshot depth:C,opaque snapshot device depth:C / ";

/// A game whose one pass, after the transparent passes, reads the snapshots it is given and draws
/// nothing.
struct Reader {
    reads: SnapshotReads,
    replaces: StockReplacement,
}

struct ReaderFeature(SnapshotReads, GameTargets);

struct Idle;

impl GameFeatureFactory for Reader {
    fn name(&self) -> &'static str {
        "post-water reader"
    }

    fn snapshot_reads(&self) -> SnapshotReads {
        self.reads
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.replaces
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(ReaderFeature(self.reads, context.targets.clone())))
    }
}

impl FrameFeature for ReaderFeature {
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
        let spec = GamePassSpec::new(GamePlace::AfterTransparent, 0)
            .writing([ColorOutput::Frame])
            .reading_snapshots(self.0);
        Ok(FeatureDeclaration {
            passes: vec![self.1.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
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

fn post_water(device: bool) -> SnapshotReads {
    SnapshotReads {
        post_water_depth: if device {
            PostWaterDepth::LinearAndDevice
        } else {
            PostWaterDepth::Linear
        },
        ..SnapshotReads::NONE
    }
}

/// The first frame of a scene with or without water and reflections, drawn with a game that
/// declares `replaces` and reads `reads`. A game that reads nothing is an inventory that only
/// declares, and with neither there is no game.
fn planned_frame(
    request: Option<SceneSsr>,
    water: bool,
    replaces: StockReplacement,
    reads: SnapshotReads,
) -> Frame {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = rig_configured(&context, scene_with(request, water), |settings| {
        if reads.any() {
            settings.game_passes = GamePasses::new().with(Arc::new(Reader { reads, replaces }));
        } else if replaces != StockReplacement::NONE {
            settings.game_passes = declaring_stock(replaces);
        }
    });
    rig.frame(0, 0)
}

fn passes_after(frame: &Frame, slot: &str) -> Vec<String> {
    frame
        .passes
        .iter()
        .filter(|pass| pass.starts_with(slot))
        .cloned()
        .collect()
}

#[test]
fn gpu_stock_water_and_stock_reflections_plan_the_post_water_depth_and_nothing_else_does() {
    let none = SnapshotReads::NONE;
    let cases = [
        ("stock water", None, true, StockReplacement::NONE, true),
        ("declared water", None, true, StockReplacement::WATER, false),
        ("no water", None, false, StockReplacement::NONE, false),
        (
            "declared water with reflections",
            Some(reflections()),
            true,
            StockReplacement::WATER,
            true,
        ),
        (
            "reflections without water",
            Some(reflections()),
            false,
            StockReplacement::NONE,
            true,
        ),
    ];
    for (label, request, water, draws, planned) in cases {
        let frame = planned_frame(request, water, draws, none);
        assert_eq!(
            passes_after(&frame, "AfterWaterDepth"),
            if planned {
                vec![LINEAR.to_owned()]
            } else {
                vec![]
            },
            "{label}: {:?}",
            frame.passes
        );
    }
}

#[test]
fn gpu_a_game_that_declares_the_post_water_depth_gets_it_planned_under_declared_water() {
    for draws in [StockReplacement::WATER, StockReplacement::NONE] {
        let linear = planned_frame(None, true, draws, post_water(false));
        assert_eq!(
            passes_after(&linear, "AfterWaterDepth"),
            [LINEAR],
            "{draws:?}: the linear depth alone adds no attachment"
        );
        let both = planned_frame(None, true, draws, post_water(true));
        assert_eq!(
            passes_after(&both, "AfterWaterDepth"),
            [WITH_DEVICE],
            "{draws:?}: the device depth is a second attachment of the same pass"
        );
        assert_eq!(
            passes_after(&both, "TransparentWater").len(),
            usize::from(!draws.water),
            "{draws:?}: only the stock draws record a water pass"
        );
    }
}

#[test]
fn gpu_a_game_that_reads_only_the_opaque_depths_leaves_the_post_water_depth_unplanned() {
    let opaque = SnapshotReads {
        linear_depth: true,
        device_depth: true,
        ..SnapshotReads::NONE
    };
    let frame = planned_frame(None, true, StockReplacement::WATER, opaque);
    assert_eq!(
        passes_after(&frame, "AfterWaterDepth"),
        Vec::<String>::new()
    );
    assert_eq!(passes_after(&frame, "AfterOpaqueDepth"), [OPAQUE_DEPTHS]);
}

#[test]
fn gpu_a_game_that_reads_the_post_water_device_depth_leaves_the_stock_passes_alone() {
    let stock = planned_frame(None, true, StockReplacement::NONE, SnapshotReads::NONE);
    let reading = planned_frame(None, true, StockReplacement::NONE, post_water(true));
    let without_game: Vec<_> = stock.passes.clone();
    let with_game: Vec<_> = reading
        .passes
        .iter()
        .filter(|pass| !pass.starts_with("Game"))
        .map(|pass| pass.replace(WITH_DEVICE, LINEAR))
        .collect();
    assert_eq!(with_game, without_game);
}
