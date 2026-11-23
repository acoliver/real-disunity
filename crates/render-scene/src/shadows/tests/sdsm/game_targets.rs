//! A game's features and targets inside a same-frame fitted capture: the capture's suffix is the
//! full frame, so what the game's passes write must be part of the capture's write contract, and
//! a suffix that fails publishes nothing of the game's bookkeeping.
use super::*;
use crate::compositor::{
    ColorOutput, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses, GameTargets,
    TargetDeclaration, TargetExtent,
};
use crate::offscreen::OffscreenTarget;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use worldrunner_render_gpu::submission::capture::WriteKey;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedFeature,
};

const SCRATCH: &str = "game scratch";
const KEPT: &str = "game kept";

/// A game that clears one layer of each target it declares and draws nothing. `poison` makes its
/// commit stage a write the capture did not declare, which fails the suffix after the game
/// staged its own bookkeeping. `layer` is the layer of each target the next frame clears.
struct Clears {
    targets: Vec<TargetDeclaration>,
    poison: Arc<AtomicBool>,
    layer: Arc<AtomicU32>,
}

impl GameFeatureFactory for Clears {
    fn name(&self) -> &'static str {
        "clears"
    }
    fn targets(&self) -> Vec<TargetDeclaration> {
        self.targets.clone()
    }
    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(ClearsFeature {
            targets: context.targets.clone(),
            writes: self.targets.clone(),
            poison: self.poison.clone(),
            layer: self.layer.clone(),
        }))
    }
}

struct ClearsFeature {
    targets: GameTargets,
    writes: Vec<TargetDeclaration>,
    poison: Arc<AtomicBool>,
    layer: Arc<AtomicU32>,
}

impl FrameFeature for ClearsFeature {
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
        let mut declared = Vec::new();
        for (occurrence, target) in (0..).zip(&self.writes) {
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, occurrence).writing([
                ColorOutput::Target(
                    target.layer(self.layer.load(Ordering::SeqCst).min(target.layers - 1)),
                ),
            ]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }
    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(ClearsFrame {
            poison: self.poison.clone(),
        }))
    }
}

struct ClearsFrame {
    poison: Arc<AtomicBool>,
}

impl PreparedFeature for ClearsFrame {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut PrepareContext<'_>) {
        if self.poison.load(Ordering::SeqCst) {
            let _ = context
                .publications()
                .stage_write(WriteKey::Resident(1), || {});
        }
    }
    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

fn scratch() -> TargetDeclaration {
    TargetDeclaration::new(
        SCRATCH,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    )
}

fn kept() -> TargetDeclaration {
    TargetDeclaration::new(
        KEPT,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    )
    .persistent()
}

/// A shadowed scene whose own renderer carries the game and fits shadows by same-frame capture.
fn captured(
    context: &GpuContext,
    targets: Vec<TargetDeclaration>,
) -> (
    ShadowedScene,
    crate::compositor::FrameRenderer,
    OffscreenTarget,
    Arc<AtomicBool>,
) {
    let (renderer, scheduler, target, poison, _) = captured_layers(context, targets);
    (renderer, scheduler, target, poison)
}

fn captured_layers(
    context: &GpuContext,
    targets: Vec<TargetDeclaration>,
) -> (
    ShadowedScene,
    crate::compositor::FrameRenderer,
    OffscreenTarget,
    Arc<AtomicBool>,
    Arc<AtomicU32>,
) {
    let poison = Arc::new(AtomicBool::new(false));
    let layer = Arc::new(AtomicU32::new(0));
    let game = Arc::new(Clears {
        targets,
        poison: poison.clone(),
        layer: layer.clone(),
    });
    let (renderer, scheduler, target) = captured_with(context, game);
    (renderer, scheduler, target, poison, layer)
}

fn captured_with(
    context: &GpuContext,
    game: Arc<dyn GameFeatureFactory>,
) -> (
    ShadowedScene,
    crate::compositor::FrameRenderer,
    OffscreenTarget,
) {
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = shadowed(context, &source, settings()).unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    GamePasses::new()
        .with(game)
        .install(&mut scheduler, context, None)
        .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    (renderer, scheduler, target)
}

#[test]
fn a_game_feature_that_writes_nothing_is_admitted_by_an_owned_capture() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, mut scheduler, target, _) = captured(&context, Vec::new());
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(renderer.last_depth_fit().is_some(), "the capture fitted");
}

#[test]
fn a_clear_only_game_target_is_written_inside_a_same_frame_capture() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, mut scheduler, target, _) = captured(&context, vec![scratch(), kept()]);
    assert!(!scheduler.game_targets().is_written(KEPT, 0));
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(renderer.last_depth_fit().is_some(), "the capture fitted");
    assert!(
        scheduler.game_targets().is_written(KEPT, 0),
        "the submitted capture frame wrote the persistent target"
    );
}

#[test]
fn a_capture_whose_suffix_fails_publishes_none_of_the_games_bookkeeping() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, mut scheduler, target, poison) = captured(&context, vec![kept()]);
    poison.store(true, Ordering::SeqCst);
    assert!(render_target(&mut renderer, &mut scheduler, &context, &view(), &target).is_err());
    assert!(
        !scheduler.game_targets().is_written(KEPT, 0),
        "a failed suffix leaves the persistent target unwritten"
    );
    poison.store(false, Ordering::SeqCst);
    render_target(&mut renderer, &mut scheduler, &context, &view(), &target).unwrap();
    assert!(scheduler.game_targets().is_written(KEPT, 0));
}

#[test]
fn a_persistent_game_target_written_by_an_earlier_frame_is_captured_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut renderer, mut scheduler, target, _) = captured(&context, vec![scratch(), kept()]);
    for frame in 0..3 {
        render_target(&mut renderer, &mut scheduler, &context, &view(), &target)
            .unwrap_or_else(|error| panic!("capture {frame}: {error:?}"));
        assert!(
            scheduler.game_targets().is_written(KEPT, 0),
            "capture {frame} left the persistent target written"
        );
    }
}

#[test]
fn a_capture_that_attaches_only_unwritten_layers_of_a_written_persistent_target_succeeds() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let rotating = TargetDeclaration::new(
        KEPT,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    )
    .layers(3)
    .persistent();
    let (mut renderer, mut scheduler, target, _, layer) =
        captured_layers(&context, vec![scratch(), rotating]);
    // Each capture clears the next layer only, so an earlier capture's layer is never attached
    // by the frame that follows it while the texture already holds that capture's contents.
    for index in 0..3 {
        layer.store(index, Ordering::SeqCst);
        render_target(&mut renderer, &mut scheduler, &context, &view(), &target)
            .unwrap_or_else(|error| panic!("capture of layer {index}: {error:?}"));
        for written in 0..=index {
            assert!(
                scheduler.game_targets().is_written(KEPT, written),
                "layer {written} after the capture of layer {index}"
            );
        }
    }
}

/// A game that always writes layer 0 of the persistent target and, once a submitted frame has
/// written it, also plans a pass that samples it. `planned` records how many passes each plan of
/// the feature declared.
struct Samples {
    planned: Arc<Mutex<Vec<usize>>>,
}

impl GameFeatureFactory for Samples {
    fn name(&self) -> &'static str {
        "samples"
    }
    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![scratch(), kept()]
    }
    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(SamplesFeature {
            targets: context.targets.clone(),
            planned: self.planned.clone(),
        }))
    }
}

struct SamplesFeature {
    targets: GameTargets,
    planned: Arc<Mutex<Vec<usize>>>,
}

impl FrameFeature for SamplesFeature {
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
        let (scratch, kept) = (scratch(), kept());
        let write = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
            .writing([ColorOutput::Target(kept.layer(0))]);
        let mut declared = vec![self.targets.plan_pass(&write, passes, views)?];
        if self.targets.is_written(KEPT, 0) {
            let sample = GamePassSpec::new(GamePlace::BeforeOpaque, 1)
                .writing([ColorOutput::Target(scratch.layer(0))])
                .reading([kept.layer(0)]);
            declared.push(self.targets.plan_pass(&sample, passes, views)?);
        }
        self.planned.lock().unwrap().push(declared.len());
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }
    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(ClearsFrame {
            poison: Arc::new(AtomicBool::new(false)),
        }))
    }
}

#[test]
fn a_feature_that_samples_a_target_once_it_is_written_plans_it_from_the_frame_after_the_write() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let planned = Arc::new(Mutex::new(Vec::new()));
    let (mut renderer, mut scheduler, target) = captured_with(
        &context,
        Arc::new(Samples {
            planned: planned.clone(),
        }),
    );
    for frame in 0..3 {
        render_target(&mut renderer, &mut scheduler, &context, &view(), &target)
            .unwrap_or_else(|error| panic!("capture {frame}: {error:?}"));
    }
    let planned = planned.lock().unwrap().clone();
    // The sampling pass joins from the frame after the one that wrote the target.
    assert_eq!(planned.first(), Some(&1), "the first frame writes only");
    assert_eq!(
        planned.last(),
        Some(&2),
        "a later frame samples what the first one wrote: {planned:?}"
    );
}
