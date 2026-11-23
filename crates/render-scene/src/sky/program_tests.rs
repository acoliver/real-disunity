//! The celestial draw on render-program: a body draws through the scene's object runtime, with
//! programs built for the keys it uses, instead of the 32 fixed pipelines the sky builds per
//! selection.

use super::capture_tests::{Case, render, scene, view};
use super::meshes::Recorder;
use crate::test_support::upload_scene;
use crate::{device::GpuContext, frame::FrameUniform, renderer::ScenePipelines};

/// Pipelines the scene's object runtime built for one frame of `case`.
fn pipelines_built(context: &GpuContext, case: &Case) -> u64 {
    let device = context.device();
    let source = scene(case);
    let pipelines = ScenePipelines::new(device, wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let mut gpu = upload_scene(device, context.submission(), &source, &pipelines).unwrap();
    let frame = FrameUniform::new(&view(false), source.effects()).unwrap();
    drop(render(
        context,
        (&mut gpu, &pipelines),
        &frame,
        Recorder::Live,
    ));
    pipelines.object_runtime_stats().pipelines_built
}

/// One body with one material key draws through one program of the scene's object runtime.
/// The fixed path built its own 32 pipelines outside the runtime and added none to it.
#[test]
fn a_celestial_body_draws_through_one_object_runtime_pipeline() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let with_body = Case::base();
    let mut without_body = Case::base();
    without_body.sky.meshes.clear();
    let (with, without) = (
        pipelines_built(&context, &with_body),
        pipelines_built(&context, &without_body),
    );
    assert_eq!(
        with,
        without + 1,
        "the body adds one runtime pipeline (with {with}, without {without})"
    );
}
