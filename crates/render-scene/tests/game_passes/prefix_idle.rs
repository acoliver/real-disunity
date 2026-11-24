//! A depth-prefix feature that plans no pass takes no part in the prefix: the frame prepares,
//! commits and finishes it as an ordinary feature, once per frame, whether or not a route
//! submits the prefix ahead of the frame.
use super::fixture::{Rig, SIZE, quad_scene};
use super::shadowed::{capture, shadowed};
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PrepareContext,
    PreparedFeature,
};
use worldrunner_render_scene::{
    compositor::{GameBuild, GameFeatureFactory, GamePasses},
    wgpu,
};

/// How often the frames prepared, committed and finished the depth-prefix feature.
type Calls = Arc<Mutex<[u32; 3]>>;

struct Idle {
    calls: Calls,
}

impl GameFeatureFactory for Idle {
    fn name(&self) -> &'static str {
        "idle prefix"
    }

    fn build(&self, _context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(Counter { calls: None }))
    }

    fn build_depth_prefix(
        &self,
        _context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        Ok(Some(Box::new(Counter {
            calls: Some(self.calls.clone()),
        })))
    }
}

/// Plans nothing; the depth-prefix one counts its preparations, commits and finished draws, as
/// a feature that stages work there without drawing would.
#[derive(Clone)]
struct Counter {
    calls: Option<Calls>,
}

impl Counter {
    fn count(&self, index: usize) {
        if let Some(calls) = &self.calls {
            calls.lock().unwrap()[index] += 1;
        }
    }
}

impl FrameFeature for Counter {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Counter {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.count(0);
        Ok(())
    }

    fn commit(&mut self, _context: &mut PrepareContext<'_>) {
        self.count(1);
    }

    fn finish_draws(&mut self, _context: &mut PrepareContext<'_>) {
        self.count(2);
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {
        unreachable!("the feature declares no pass")
    }
}

fn inventory() -> (GamePasses, Calls) {
    let calls = Calls::default();
    let inventory = GamePasses::new().with(Arc::new(Idle {
        calls: calls.clone(),
    }));
    (inventory, calls)
}

#[test]
fn a_frame_drawn_whole_prepares_a_depth_prefix_feature_that_plans_no_pass() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, calls) = inventory();
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    rig.render().expect("the first frame renders");
    rig.render().expect("the second frame renders");
    assert_eq!(
        *calls.lock().unwrap(),
        [2, 2, 2],
        "prepared, committed and finished once per frame"
    );
}

#[test]
fn a_separately_submitted_prefix_leaves_a_feature_that_plans_no_pass_to_the_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (inventory, calls) = inventory();
    let (mut rig, mut scene) = shadowed(inventory);
    capture(&mut rig, &mut scene, 0);
    capture(&mut rig, &mut scene, 1);
    assert_eq!(
        *calls.lock().unwrap(),
        [2, 2, 2],
        "once per frame, in the rest of the frame and not also in the prefix"
    );
}
