//! The pass list each stock frame submits, pinned for every path the frame plan is built for.
//!
//! The pins were taken from the frame before its plan was built in one place, and the plan
//! builder must reproduce them exactly: the entries, their order, the attachments each names
//! with how it loads them, the effects the planned features declare and where operations are
//! positioned. A change to any of them is a change to the frame, not to the builder.
mod pins;

use super::*;
use crate::compositor::plan_trace::{self, SubmittedPlan, assert_pinned};
use worldrunner_render_gpu::submission::PrepareError;

fn unfitted_frame(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
) -> Vec<SubmittedPlan> {
    let (color, depth) = attachments(frame);
    plan_trace::take();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode_with(context, &view(), (&color, &depth), encoder, Some(frame))
        })
        .map_err(PrepareError::into_error)
        .unwrap();
    prepared.submit().unwrap();
    plan_trace::take()
}

fn fitted_plans(
    context: &GpuContext,
    frame: &mut FrameRenderer,
    renderer: &mut ShadowedScene,
    adoption: AdoptionPolicy,
) -> Vec<SubmittedPlan> {
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: adoption,
                range: policy(),
            }),
        )
        .unwrap();
    let (color, depth) = attachments(frame);
    plan_trace::take();
    renderer
        .render_depth_fitted_with(context, &view(), (&color, &depth), 3, frame)
        .unwrap();
    plan_trace::take()
}

const SAME_FRAME: AdoptionPolicy = AdoptionPolicy::SameFrameCapture;
const INTERACTIVE: AdoptionPolicy = AdoptionPolicy::Interactive {
    latency_frames: 2,
    contraction_frames: 0,
};

fn with_path(path: SceneOpaquePath) -> RenderScene {
    let base = fitting_source();
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(path)
        .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: base.terrain().cloned(),
        effects: Some(effects),
    })
    .unwrap()
}

/// Renderers for `source` with no depth fitting set yet.
fn renderers(context: &GpuContext, source: &RenderScene) -> (FrameRenderer, ShadowedScene) {
    let size = [view().viewport().width, view().viewport().height];
    let frame = FrameRenderer::hdr(
        context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let mut shadows = settings();
    shadows.normal_bias_meters = 0.0;
    let renderer = ShadowedScene::with_target(
        context,
        source,
        shadows,
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    (frame, renderer)
}

fn screen_plans(adoption: Option<AdoptionPolicy>) -> Vec<SubmittedPlan> {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let (mut frame, mut renderer) = crate::shadows::tests::sdsm::screen_renderers(&context);
    match adoption {
        Some(adoption) => fitted_plans(&context, &mut frame, &mut renderer, adoption),
        None => unfitted_frame(&context, &mut frame, &mut renderer),
    }
}

fn path_plans(path: SceneOpaquePath, adoption: Option<AdoptionPolicy>) -> Vec<SubmittedPlan> {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = with_path(path);
    let (mut frame, mut renderer) = renderers(&context, &source);
    match adoption {
        Some(adoption) => fitted_plans(&context, &mut frame, &mut renderer, adoption),
        None => unfitted_frame(&context, &mut frame, &mut renderer),
    }
}

#[test]
fn gpu_screen_effects_frame_without_fitting_submits_its_pinned_plan() {
    let plans = screen_plans(None);
    assert_pinned("screen effects, unfitted", &plans, pins::SCREEN_UNFITTED);
}

#[test]
fn gpu_screen_effects_frame_with_same_frame_fitting_submits_its_pinned_plans() {
    let plans = screen_plans(Some(SAME_FRAME));
    assert_pinned(
        "screen effects, same frame",
        &plans,
        pins::SCREEN_SAME_FRAME,
    );
}

#[test]
fn gpu_screen_effects_frame_with_interactive_fitting_submits_its_pinned_plans() {
    let plans = screen_plans(Some(INTERACTIVE));
    assert_pinned(
        "screen effects, interactive",
        &plans,
        pins::SCREEN_INTERACTIVE,
    );
}

#[test]
fn gpu_forward_frame_without_fitting_submits_its_pinned_plan() {
    let plans = path_plans(SceneOpaquePath::Forward, None);
    assert_pinned("forward, unfitted", &plans, pins::FORWARD_UNFITTED);
}

#[test]
fn gpu_forward_frame_with_same_frame_fitting_submits_its_pinned_plans() {
    let plans = path_plans(SceneOpaquePath::Forward, Some(SAME_FRAME));
    assert_pinned("forward, same frame", &plans, pins::FORWARD_SAME_FRAME);
}

#[test]
fn gpu_forward_frame_with_interactive_fitting_submits_its_pinned_plans() {
    let plans = path_plans(SceneOpaquePath::Forward, Some(INTERACTIVE));
    assert_pinned("forward, interactive", &plans, pins::FORWARD_INTERACTIVE);
}

#[test]
fn gpu_gbuffer_observation_frame_without_fitting_submits_its_pinned_plan() {
    let plans = path_plans(SceneOpaquePath::GBUFFER_OBSERVATION, None);
    assert_pinned("observation, unfitted", &plans, pins::OBSERVATION_UNFITTED);
}

#[test]
fn gpu_gbuffer_observation_frame_with_same_frame_fitting_submits_its_pinned_plans() {
    let plans = path_plans(SceneOpaquePath::GBUFFER_OBSERVATION, Some(SAME_FRAME));
    assert_pinned(
        "observation, same frame",
        &plans,
        pins::OBSERVATION_SAME_FRAME,
    );
}

#[test]
fn gpu_gbuffer_observation_frame_with_interactive_fitting_submits_its_pinned_plans() {
    let plans = path_plans(SceneOpaquePath::GBUFFER_OBSERVATION, Some(INTERACTIVE));
    assert_pinned(
        "observation, interactive",
        &plans,
        pins::OBSERVATION_INTERACTIVE,
    );
}

#[test]
fn gpu_gbuffer_render_frame_without_fitting_submits_its_pinned_plan() {
    let plans = path_plans(SceneOpaquePath::GBUFFER_RENDER, None);
    assert_pinned("render, unfitted", &plans, pins::RENDER_UNFITTED);
}

#[test]
fn gpu_gbuffer_render_frame_with_the_fused_prepass_and_same_frame_fitting_submits_its_pinned_plans()
{
    let plans = path_plans(SceneOpaquePath::GBUFFER_RENDER, Some(SAME_FRAME));
    assert_pinned("render, same frame", &plans, pins::RENDER_SAME_FRAME);
}

#[test]
fn gpu_gbuffer_render_frame_with_interactive_fitting_submits_its_pinned_plans() {
    let plans = path_plans(SceneOpaquePath::GBUFFER_RENDER, Some(INTERACTIVE));
    assert_pinned("render, interactive", &plans, pins::RENDER_INTERACTIVE);
}
