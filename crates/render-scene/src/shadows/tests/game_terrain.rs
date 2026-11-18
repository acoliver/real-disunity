//! A game feature on the shadowed route reads the frame's streamed terrain like on the
//! unshadowed one.
use super::*;
use crate::compositor::{FrameTerrainHandle, GameBuild, GameFeatureFactory, GamePasses};
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PrepareContext, PreparedFeature,
};

thread_local! {
    /// The handle the probe was built with, kept on the test's thread to read between frames.
    static HANDLE: std::cell::RefCell<Option<FrameTerrainHandle>> =
        const { std::cell::RefCell::new(None) };
}

/// The number of chunks each prepared frame lent.
type Lent = Arc<Mutex<Vec<usize>>>;

struct Probe {
    lent: Lent,
}

impl GameFeatureFactory for Probe {
    fn name(&self) -> &'static str {
        "terrain probe"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        HANDLE.with_borrow_mut(|handle| *handle = Some(context.terrain.clone()));
        Ok(Box::new(ProbeFeature {
            terrain: context.terrain.clone(),
            lent: self.lent.clone(),
        }))
    }
}

#[derive(Clone)]
struct ProbeFeature {
    terrain: FrameTerrainHandle,
    lent: Lent,
}

impl FrameFeature for ProbeFeature {
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
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for ProbeFeature {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let chunks = self
            .terrain
            .current()?
            .expect("the scene streams its terrain");
        self.lent.lock().unwrap().push(chunks.len());
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

#[test]
fn shadowed_frames_lend_their_streamed_terrain_to_game_features() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = super::streaming::multilevel_source();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let lent = Lent::default();
    GamePasses::new()
        .with(Arc::new(Probe { lent: lent.clone() }))
        .install(&mut renderer.scheduler, &context)
        .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for _ in 0..2 {
        render_target(&mut renderer, &context, &view(), &target).unwrap();
    }
    let lent = lent.lock().unwrap();
    assert_eq!(lent.len(), 2, "the probe prepared in both frames");
    assert!(lent.iter().all(|&count| count > 0));
    HANDLE.with_borrow(|handle| assert!(handle.as_ref().unwrap().current().is_err()));
}
