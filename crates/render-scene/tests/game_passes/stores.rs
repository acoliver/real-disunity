//! What a frame leaves in a persistent target: written layers are those whose last attachment
//! stored its contents, of the exact texture the game declared. A stock attachment that shares a
//! declared target's name is a different texture and never marks it.
use super::fixture::{FORMAT, Rig, SIZE, quad_scene};
use std::sync::{Arc, Mutex};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PrepareContext, PreparedFeature, ResourceRef,
};
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, GameBuild, GameFeatureFactory, GamePassSpec, GamePasses, GameTargets,
        TargetDeclaration, TargetExtent,
    },
    wgpu,
};

const PAD: &str = "pad";
/// A stock attachment's name, which a game may also choose for a target of its own.
const STOCK_NAME: &str = "scene color";

#[derive(Default)]
pub struct Plan {
    /// Passes that clear the pad, each ending with the given store.
    pub stores: Vec<wgpu::StoreOp>,
    /// A pass that samples this target.
    pub read: Option<&'static str>,
}

type Shared = Arc<Mutex<Plan>>;

struct Pads {
    plan: Shared,
    declarations: Vec<TargetDeclaration>,
}

impl GameFeatureFactory for Pads {
    fn name(&self) -> &'static str {
        "pads"
    }
    fn targets(&self) -> Vec<TargetDeclaration> {
        self.declarations.clone()
    }
    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(PadsFeature {
            targets: context.targets.clone(),
            plan: self.plan.clone(),
        }))
    }
}

struct PadsFeature {
    targets: GameTargets,
    plan: Shared,
}

impl FrameFeature for PadsFeature {
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
        let plan = self.plan.lock().unwrap();
        let mut declared = Vec::new();
        for (occurrence, store) in (0..).zip(&plan.stores) {
            let spec = GamePassSpec::new(GamePlace::BeforeOpaque, occurrence)
                .writing([ColorOutput::Target(ResourceRef::new(PAD))]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
            passes.last_mut().unwrap().color.as_mut().unwrap().store = *store;
        }
        if let Some(name) = plan.read {
            let spec = GamePassSpec::new(GamePlace::AfterOpaque, 0)
                .writing([ColorOutput::Frame])
                .reading([ResourceRef::new(name)]);
            declared.push(self.targets.plan_pass(&spec, passes, views)?);
        }
        Ok(FeatureDeclaration {
            passes: declared,
            effects: Vec::new(),
        })
    }
    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(PadsFrame))
    }
}

struct PadsFrame;

impl PreparedFeature for PadsFrame {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

fn pad() -> TargetDeclaration {
    TargetDeclaration::new(
        PAD,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    )
    .persistent()
}

/// A persistent target that follows the frame and is named like the stock colour attachment.
fn stock_named() -> TargetDeclaration {
    TargetDeclaration::new(STOCK_NAME, FORMAT, TargetExtent::Frame).persistent()
}

fn rig(declarations: Vec<TargetDeclaration>) -> (Rig, Shared) {
    let plan = Shared::default();
    let inventory = GamePasses::new().with(Arc::new(Pads {
        plan: plan.clone(),
        declarations,
    }));
    let rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    (rig, plan)
}

#[test]
fn a_layer_whose_last_attachment_discarded_its_contents_is_not_retained_state() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, plan) = rig(vec![pad()]);
    plan.lock().unwrap().stores = vec![wgpu::StoreOp::Discard];
    rig.render().unwrap();
    assert!(!rig.renderer.game_targets().is_written(PAD, 0));

    // Stored by one pass and discarded by a later one of the same frame: the contents are gone.
    plan.lock().unwrap().stores = vec![wgpu::StoreOp::Store, wgpu::StoreOp::Discard];
    rig.render().unwrap();
    assert!(!rig.renderer.game_targets().is_written(PAD, 0));

    plan.lock().unwrap().stores = Vec::new();
    plan.lock().unwrap().read = Some(PAD);
    let Err(error) = rig.render() else {
        panic!("nothing wrote the pad");
    };
    assert!(
        matches!(error, FrameError::MissingProducer(resource) if resource.name == PAD),
        "{error:?}"
    );

    plan.lock().unwrap().read = None;
    plan.lock().unwrap().stores = vec![wgpu::StoreOp::Store];
    rig.render().unwrap();
    assert!(
        rig.renderer.game_targets().is_written(PAD, 0),
        "a stored layer is retained state"
    );

    // Written earlier, then discarded: the layer no longer holds anything to read.
    plan.lock().unwrap().stores = vec![wgpu::StoreOp::Discard];
    rig.render().unwrap();
    assert!(!rig.renderer.game_targets().is_written(PAD, 0));
}

#[test]
fn a_target_named_like_a_stock_attachment_is_not_marked_by_the_stock_pass_that_writes_it() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, plan) = rig(vec![stock_named()]);
    rig.render().unwrap();
    assert!(
        !rig.renderer.game_targets().is_written(STOCK_NAME, 0),
        "the stock Opaque pass wrote a different texture"
    );
    plan.lock().unwrap().read = Some(STOCK_NAME);
    let Err(error) = rig.render() else {
        panic!("the game's own target holds nothing");
    };
    assert!(
        matches!(error, FrameError::AttachmentHazard(resource) if resource.name == STOCK_NAME),
        "{error:?}"
    );
}
