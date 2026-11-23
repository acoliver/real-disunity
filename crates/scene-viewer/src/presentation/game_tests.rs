//! The viewer carries the game's inventory into every renderer it builds, without knowing what the
//! game draws.
use super::*;
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PrepareContext, PreparedFeature,
};
use worldrunner_render_scene::compositor::{
    GameBuild, GameFeatureFactory, GamePasses, TargetDeclaration, TargetExtent,
};

/// A game that owns one 64 KiB target and draws nothing, counting how often it is built.
struct Counting {
    builds: Arc<Mutex<u32>>,
}

struct Nothing;
impl FrameFeature for Nothing {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }
    fn plan(
        &self,
        _passes: &mut Vec<PassDeclaration>,
        _views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        Ok(self.declare())
    }
    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Nothing))
    }
}
impl PreparedFeature for Nothing {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

impl GameFeatureFactory for Counting {
    fn name(&self) -> &'static str {
        "counting game"
    }
    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![
            TargetDeclaration::new(
                "game lookup",
                wgpu::TextureFormat::Rgba8Unorm,
                TargetExtent::Fixed([128, 128]),
            )
            .persistent(),
        ]
    }
    fn build(&self, _context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        *self.builds.lock().unwrap() += 1;
        Ok(Box::new(Nothing))
    }
}

fn game(builds: &Arc<Mutex<u32>>) -> crate::GameRendering {
    crate::GameRendering {
        passes: GamePasses::new().with(Arc::new(Counting {
            builds: builds.clone(),
        })),
    }
}

fn policy(game: crate::GameRendering) -> PresentationPolicy {
    PresentationPolicy {
        receiver_gradients:
            worldrunner_render_scene::compositor::ReceiverGradientReservation::Opposing,
        budget: crate::PresentationBudget::default().into(),
        capture_pool: crate::CapturePoolPolicy::default(),
        recipes: false,
        game,
    }
}

#[test]
fn the_inventory_counts_the_games_targets_and_carries_its_passes() {
    let size = Viewport {
        width: 65,
        height: 49,
    };
    let limits = wgpu::Limits::default();
    let settings =
        RendererConfiguration::new([65, 49], COLOR, worldrunner_render_post::ToneMap::Clamp);
    let (plain, _) = policy(crate::GameRendering::default())
        .inventory(settings.clone(), size, &limits)
        .unwrap();
    let builds = Arc::new(Mutex::new(0));
    let (with_game, _) = policy(game(&builds))
        .inventory(settings, size, &limits)
        .unwrap();
    let bytes =
        |settings: &RendererConfiguration| settings.allocations().validate(&limits).unwrap();
    assert_eq!(bytes(&with_game) - bytes(&plain), 128 * 128 * 4);
    assert_eq!(with_game.game_passes.len(), 1);
    assert!(plain.game_passes.is_empty());
    assert_eq!(*builds.lock().unwrap(), 0, "planning builds nothing");
}

#[test]
fn every_renderer_the_viewer_builds_registers_the_games_factory() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = worldrunner_render_scene::device::GpuContext::shared_for_test().unwrap();
    let size = Viewport {
        width: 65,
        height: 49,
    };
    let builds = Arc::new(Mutex::new(0));
    let mut presentation = Presentation::new(
        &context,
        COLOR,
        [size, size],
        crate::tests::CLAMP,
        None,
        &policy(game(&builds)),
    )
    .unwrap();
    assert_eq!(*builds.lock().unwrap(), 1, "registered on allocation");
    // A reconfiguration derived from the live one keeps the inventory and registers again.
    let mut settings = presentation.scheduler.configuration().clone();
    settings.ui_depth = true;
    presentation
        .scheduler
        .reconfigure(&context, settings)
        .unwrap();
    assert_eq!(*builds.lock().unwrap(), 2, "registered after replacement");
}
