//! The object shadow casters record through the program runtime of the receivers' family.

use super::*;
use crate::offscreen::OffscreenTarget;

/// The object pipelines the family of `renderer` has built after one frame of `camera`.
fn built_after_one_frame(
    context: &GpuContext,
    source: &RenderScene,
    camera: &SceneView,
    shadows: bool,
) -> u64 {
    let mut renderer = shadowed(context, source, settings()).unwrap();
    renderer.set_shadows_enabled(shadows);
    let target = OffscreenTarget::new(context.device(), camera.viewport()).unwrap();
    let mut scheduler = crate::compositor::FrameRenderer::new(context.device());
    render_target(&mut renderer, &mut scheduler, context, camera, &target).unwrap();
    renderer
        .scene_and_pipelines_mut()
        .1
        .object_runtime_stats()
        .pipelines_built
}

/// A frame whose cascades draw the casters builds the casters' depth-only pipelines on the same
/// object runtime as the receivers, on top of what a frame without shadow passes builds.
#[test]
fn object_casters_build_their_pipelines_on_the_receivers_program_runtime() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let camera = view();
    let lit = built_after_one_frame(&context, &source, &camera, false);
    let shadowed = built_after_one_frame(&context, &source, &camera, true);
    assert!(
        lit > 0,
        "the receivers draw the scene's objects on the program runtime"
    );
    assert!(
        shadowed > lit,
        "the cascades built no object caster pipeline on the program runtime (lit {lit}, \
         shadowed {shadowed})"
    );
}
