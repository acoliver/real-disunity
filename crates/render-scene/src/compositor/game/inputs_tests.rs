//! The lent frame inputs outside the stock frame routes: the geometry-only `encode` lends the
//! occlusion and environment it has, which is none, and a depth prefix whose game passes cannot
//! be placed leaves no plan behind for the next prefix.
use super::{FrameEnvironmentHandle, FrameInputError, FrameOcclusionHandle};
use super::{GameBuild, GameFeatureFactory, GamePasses};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration, attachment};
use crate::device::GpuContext;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FeaturePass, FrameError,
    FrameFeature, GamePlace, PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature,
    ResourceRef,
};

const EXTENT: [u32; 2] = [16, 16];

/// Whether each prepared frame was lent an occlusion and an environment.
type Seen = Arc<Mutex<Vec<(Result<bool, FrameInputError>, Result<bool, FrameInputError>)>>>;

/// A game whose feature records what it is lent; with `place`, its depth-prefix feature lists a
/// pass after the prepass of its own making, as a feature that does not use `plan_pass` can.
struct Probe {
    seen: Seen,
    place: bool,
}

impl GameFeatureFactory for Probe {
    fn name(&self) -> &'static str {
        "input probe"
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(Lent {
            seen: self.seen.clone(),
            occlusion: context.occlusion.clone(),
            environment: context.environment.clone(),
        }))
    }

    fn build_depth_prefix(
        &self,
        _context: &GameBuild<'_>,
    ) -> Result<Option<Box<dyn FrameFeature>>, FrameError> {
        Ok(self
            .place
            .then(|| Box::new(Placer) as Box<dyn FrameFeature>))
    }
}

#[derive(Clone)]
struct Lent {
    seen: Seen,
    occlusion: FrameOcclusionHandle,
    environment: FrameEnvironmentHandle,
}

impl FrameFeature for Lent {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for Lent {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.seen.lock().unwrap().push((
            self.occlusion.current().map(|lent| lent.is_some()),
            self.environment.current().map(|lent| lent.is_some()),
        ));
        Ok(())
    }

    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

const PLACED: PassSlot = PassSlot::Game(GamePlace::AfterDepthPrepass);

struct Placer;

impl FrameFeature for Placer {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        _: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        passes.push(PassDeclaration::new(PLACED));
        Ok(FeatureDeclaration {
            passes: vec![FeaturePass::new(PLACED)],
            effects: Vec::new(),
        })
    }

    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        unreachable!("the prefix is refused before anything prepares")
    }
}

fn renderer(context: &GpuContext, place: bool) -> (ConfiguredFrameRenderer, Seen) {
    let seen = Seen::default();
    let mut settings = RendererConfiguration::new(
        EXTENT,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        worldrunner_render_post::ToneMap::Clamp,
    );
    settings.game_passes = GamePasses::new().with(Arc::new(Probe {
        seen: seen.clone(),
        place,
    }));
    let renderer = ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 26))
        .expect("the renderer builds");
    (renderer, seen)
}

/// One geometry-only opaque pass over the renderer's scene colour and depth.
fn opaque(renderer: &ConfiguredFrameRenderer) -> (PassDeclaration, [AttachmentView; 2]) {
    let targets = renderer.targets();
    let color =
        AttachmentView::new(ResourceRef::new("scene color"), targets.scene().texture()).unwrap();
    let depth =
        AttachmentView::new(ResourceRef::new("scene depth"), targets.depth().texture()).unwrap();
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(color.resource(), &color, false));
    pass.depth = Some(attachment(depth.resource(), &depth, true));
    (pass, [color, depth])
}

#[test]
fn gpu_the_geometry_only_encode_lends_no_occlusion_and_no_environment() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let (mut renderer, seen) = renderer(&context, false);
    let (pass, views) = opaque(&renderer);
    let device = context.device();
    let frame = renderer.renderer_mut();
    context
        .submission()
        .prepare(device, |encoder, _| {
            frame.encode(device, encoder, &[pass], &views, &mut [])
        })
        .expect("the geometry-only frame encodes")
        .submit()
        .unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        [(Ok(false), Ok(false))],
        "a renderer without SSAO, reflections or water lends neither, rather than refusing"
    );
}

#[test]
fn gpu_a_prefix_whose_game_passes_cannot_be_placed_keeps_no_plan_for_the_next_prefix() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let (mut renderer, _) = renderer(&context, true);
    let frame = renderer.renderer_mut();
    let view = crate::SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        worldrunner_render_plan::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        crate::Viewport {
            width: EXTENT[0],
            height: EXTENT[1],
        },
    )
    .unwrap();
    let uniform = crate::frame::FrameUniform::new(&view, None).unwrap();
    // A prefix list without the depth prepass, which the game's pass must follow.
    let (mut passes, mut views) = (vec![PassDeclaration::new(PassSlot::Opaque)], Vec::new());
    assert_eq!(
        frame.plan_game_prefix(&uniform, &mut passes, &mut views),
        Err(FrameError::MissingPass(PassEntry::new(
            PassSlot::DepthNormalPrepass
        )))
    );
    assert!(
        !frame.chain.has_prefix_plan(),
        "the refused prefix's declarations are not kept for the next prefix"
    );
}
