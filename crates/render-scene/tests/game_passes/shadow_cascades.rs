//! The stock shadow cascades on a route that publishes none, and what the seam says about
//! reading them. The shadowed route's side is in `src/shadows/tests/game_shadows.rs`.
use super::fixture::{Rig, SIZE, quad_scene};
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, FrameInputError, FrameShadowHandle, GameBuild, GameFeatureFactory,
        GamePassSpec, GamePasses, GameTargets,
    },
    wgpu,
};

type Seen = Arc<Mutex<Vec<Result<usize, FrameInputError>>>>;

/// A game that reads the lent shadows in every frame, and with `read_stock` also plans a pass
/// that declares a read of the stock cascade layers.
struct Reader {
    seen: Seen,
    read_stock: bool,
}

impl GameFeatureFactory for Reader {
    fn name(&self) -> &'static str {
        "shadow reader"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(ReaderFeature {
            seen: self.seen.clone(),
            shadows: context.shadows.clone(),
            targets: context.targets.clone(),
            read_stock: self.read_stock,
        }))
    }
}

struct ReaderFeature {
    seen: Seen,
    shadows: FrameShadowHandle,
    targets: GameTargets,
    read_stock: bool,
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
        let mut declared = Vec::new();
        if self.read_stock {
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
                .writing([ColorOutput::Frame])
                .reading([ResourceRef::new("shadow depth")]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(ReaderFrame {
            seen: self.seen.clone(),
            shadows: self.shadows.clone(),
        }))
    }
}

struct ReaderFrame {
    seen: Seen,
    shadows: FrameShadowHandle,
}

impl PreparedFeature for ReaderFrame {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.seen
            .lock()
            .unwrap()
            .push(self.shadows.current().map(|shadows| shadows.cascades.len()));
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

fn rig(read_stock: bool) -> (Rig, Seen) {
    let seen = Seen::default();
    let game = Arc::new(Reader {
        seen: seen.clone(),
        read_stock,
    });
    let rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(game);
    });
    (rig, seen)
}

#[test]
fn an_unshadowed_route_lends_no_cascades_and_the_frame_still_renders() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, seen) = rig(false);
    for _ in 0..2 {
        rig.render().unwrap();
    }
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [Err(FrameInputError::NotPrepared); 2].as_slice()
    );
}

#[test]
fn a_game_pass_cannot_declare_a_read_of_the_stock_cascade_layers() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, _) = rig(true);
    let FrameError::AttachmentView(resource) = rig.render().unwrap_err() else {
        panic!("the seam resolves a read against the game's own targets");
    };
    assert_eq!(resource.name, "shadow depth");
}
