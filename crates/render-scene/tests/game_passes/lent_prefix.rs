//! The depth prefix route lends the scene's meshes to a game's depth-prefix feature, in a frame
//! drawn whole and inside a same-frame capture's prefix.
use super::fixture::Rig;
use super::fixture::{SIZE, quad_scene};
use super::lent::{ClearKept, Shared, semantic};
use super::lent_terrain_game::{Clear, layered_scene};
use super::lighting::ssao;
use super::shadowed::{capture_fit, effects_with, shadowed};
use std::sync::Arc;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    GamePlace, PassDeclaration, PassSlot, PrepareContext, PreparedFeature,
};
use worldrunner_render_scene::{
    compositor::{
        ColorOutput, EngineConstants, FrameSceneMeshesHandle, GameBuild, GameFeatureFactory,
        GamePassSpec, GamePasses, GameTargets, TargetDeclaration, TargetExtent,
        TerrainMaterialChannel,
    },
    wgpu,
};

const PREFIX_TARGET: &str = "lent.prefix";

fn prefix_target() -> TargetDeclaration {
    TargetDeclaration::new(
        PREFIX_TARGET,
        wgpu::TextureFormat::Rgba8Unorm,
        TargetExtent::Fixed([4, 4]),
    )
}

/// A depth-prefix feature that records what it is lent. It plans one pass after the depth
/// prepass, because the renderer prepares a prefix feature that plans none only once, if at all.
struct PrefixProbe {
    seen: Shared,
}

impl GameFeatureFactory for PrefixProbe {
    fn name(&self) -> &'static str {
        "lent prefix"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        vec![prefix_target()]
    }

    /// The terrain material arrays the prefix feature asks for count against this budget.
    fn resource_budget(&self) -> AllocationBudget {
        AllocationBudget(1 << 20)
    }

    fn build(&self, _context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(Silent))
    }

    fn build_depth_prefix(
        &self,
        context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        Ok(Some(Box::new(Prefix {
            targets: context.targets.clone(),
            meshes: context.scene_meshes.clone(),
            engine: context.engine.clone(),
            seen: self.seen.clone(),
        })))
    }
}

#[derive(Clone)]
struct Silent;

impl FrameFeature for Silent {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Silent))
    }
}

impl PreparedFeature for Silent {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

#[derive(Clone)]
struct Prefix {
    targets: GameTargets,
    meshes: FrameSceneMeshesHandle,
    engine: EngineConstants,
    seen: Shared,
}

impl FrameFeature for Prefix {
    fn declare(&self) -> FeatureDeclaration {
        Silent.declare()
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        if !passes
            .iter()
            .any(|pass| pass.slot == PassSlot::DepthNormalPrepass)
        {
            return Ok(self.declare());
        }
        let spec = GamePassSpec::new(GamePlace::AfterDepthPrepass, 0)
            .writing([ColorOutput::Target(prefix_target().layer(0))]);
        Ok(FeatureDeclaration {
            passes: vec![self.targets.plan_pass(&spec, passes, views)?],
            effects: Vec::new(),
        })
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Prefix {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let lent = format!("{:?}", self.meshes.current().map(|meshes| meshes.len()));
        let array = self.engine.material_array(
            context,
            semantic("lent.prefix_array"),
            TerrainMaterialChannel::AlbedoMetallic,
            &[0, 1],
        );
        let mut seen = self.seen.lock().unwrap();
        seen.lent.push(lent);
        seen.terrain.push(format!("{:?}", array.map(|_| ())));
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

#[test]
fn the_depth_prefix_route_lends_the_meshes_in_a_frame_drawn_whole_and_in_a_capture() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let probe =
        |seen: &Shared| GamePasses::new().with(Arc::new(PrefixProbe { seen: seen.clone() }));
    let whole = Shared::default();
    let source = quad_scene();
    let scene = source
        .with_objects_and_effects(
            source.geometry().objects().to_vec(),
            Some(effects_with(Some(ssao()))),
        )
        .unwrap();
    let mut rig = Rig::with_ssao(&scene, SIZE, ssao(), |settings| {
        settings.game_passes = probe(&whole);
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();

    let captured = Shared::default();
    let (mut rig, mut shadowed_scene) = shadowed(probe(&captured));
    capture_fit(&mut rig, &mut shadowed_scene, 0);
    assert_eq!(whole.lock().unwrap().lent, ["Ok(1)"]);
    assert_eq!(
        captured.lock().unwrap().lent,
        ["Ok(1)"],
        "inside a same-frame capture's prefix the meshes are lent as the textures are"
    );
}

#[test]
fn a_depth_prefix_feature_reads_the_terrain_arrays() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let _terrain = Clear;
    let seen = Shared::default();
    let source = layered_scene(0);
    let scene = source
        .with_objects_and_effects(Vec::new(), Some(effects_with(Some(ssao()))))
        .unwrap();
    let mut rig = Rig::with_ssao(&scene, SIZE, ssao(), |settings| {
        settings.game_passes = GamePasses::new().with(Arc::new(PrefixProbe { seen: seen.clone() }));
    });
    let mut encoder = rig
        .context
        .submission()
        .begin(rig.context.device())
        .unwrap();
    rig.encode(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    assert_eq!(
        seen.lock().unwrap().terrain,
        ["Ok(())"],
        "the prefix is lent the scene terrain as the frame is"
    );
}
