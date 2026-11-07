//! What a frame keeps alive of the quads it draws, and the refusals of a quad group the pass
//! cannot hold.

use std::sync::Weak;

use worldrunner_render_plan::SceneTexture;

use super::DynamicQuad;
use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::frame::FrameUniform;
use crate::renderer::ScenePipelines;
use crate::{CameraPose, OrthographicLens, SceneView, Viewport};

type Probe = Weak<dyn std::any::Any + Send + Sync>;

struct Staged {
    context: GpuContext,
    pipelines: ScenePipelines,
    gpu: SceneGpu,
}

fn staged(quads: &[DynamicQuad]) -> Staged {
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("actual GPU required");
    let pipelines =
        ScenePipelines::with_samples(context.device(), wgpu::TextureFormat::Rgba8UnormSrgb, 1)
            .unwrap();
    let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    let view = SceneView::orthographic(
        CameraPose {
            eye_millimeters: [0, -10_000, 0],
            target_millimeters: [0, 0, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 2.0,
            near_meters: 0.1,
            far_meters: 100.0,
        },
        Viewport {
            width: 8,
            height: 8,
        },
    )
    .unwrap();
    gpu.set_dynamic_contribution(
        context.device(),
        context.submission(),
        &pipelines,
        &[] as &[SceneTexture],
        quads,
    )
    .unwrap();
    gpu.set_frame(context.queue(), FrameUniform::unlit(&view));
    Staged {
        context,
        pipelines,
        gpu,
    }
}

fn alive(probes: &[Probe]) -> bool {
    probes.iter().all(|probe| probe.upgrade().is_some())
}

fn gone(probes: &[Probe]) -> bool {
    probes.iter().all(|probe| probe.upgrade().is_none())
}

fn two_groups() -> [DynamicQuad; 2] {
    [
        DynamicQuad::default(),
        DynamicQuad {
            rect: [-0.25, -0.25, 0.25, 0.25],
            tint: [0.0, 1.0, 0.0, 1.0],
            ..DynamicQuad::default()
        },
    ]
}

#[test]
fn staged_quads_are_held_while_staged_and_let_go_when_replaced_with_no_frame_holding_them() {
    let mut staged = staged(&two_groups());
    let probes = staged.gpu.dynamic_probes();
    assert_eq!(probes.len(), 2);
    assert!(alive(&probes));
    staged.gpu.clear_dynamic();
    assert!(gone(&probes), "nothing else holds a replaced contribution");
}

#[test]
fn a_frame_that_retained_the_quads_holds_them_after_they_are_replaced_until_it_is_cancelled() {
    let mut staged = staged(&two_groups());
    let probes = staged.gpu.dynamic_probes();
    let frame = staged
        .context
        .submission()
        .begin(staged.context.device())
        .unwrap();
    staged
        .gpu
        .retain_dynamic(&frame, &staged.pipelines)
        .unwrap();
    staged.gpu.clear_dynamic();
    assert!(alive(&probes), "the unsubmitted frame holds the quads");
    frame.finish().cancel();
    assert!(gone(&probes), "a cancelled frame gives them back at once");
}

#[test]
fn a_submitted_frame_holds_the_quads_until_the_gpu_has_completed_it() {
    let mut staged = staged(&two_groups());
    let probes = staged.gpu.dynamic_probes();
    let frame = staged
        .context
        .submission()
        .begin(staged.context.device())
        .unwrap();
    staged
        .gpu
        .retain_dynamic(&frame, &staged.pipelines)
        .unwrap();
    staged.gpu.clear_dynamic();
    frame.finish().submit().unwrap();
    assert!(alive(&probes), "submitted and not yet observed complete");
    staged
        .context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert!(gone(&probes), "a completed frame lets the quads go");
}

#[test]
fn quads_staged_for_one_pass_are_prepared_for_another_before_its_pass_opens() {
    let mut staged = staged(&two_groups());
    let hdr =
        ScenePipelines::with_samples(staged.context.device(), wgpu::TextureFormat::Rgba16Float, 4)
            .unwrap();
    let frame = staged
        .context
        .submission()
        .begin(staged.context.device())
        .unwrap();
    assert!(
        staged.gpu.retain_dynamic(&frame, &hdr).is_err(),
        "retaining refuses a pass the quads were not prepared for"
    );
    staged.gpu.prepare_dynamic(&frame, &hdr).unwrap();
    staged.gpu.retain_dynamic(&frame, &hdr).unwrap();
    staged
        .gpu
        .retain_dynamic(&frame, &staged.pipelines)
        .unwrap();
    frame.finish().cancel();
}

#[test]
fn a_frame_without_quads_retains_and_prepares_nothing() {
    let mut staged = staged(&[]);
    let frame = staged
        .context
        .submission()
        .begin(staged.context.device())
        .unwrap();
    staged
        .gpu
        .prepare_dynamic(&frame, &staged.pipelines)
        .unwrap();
    staged
        .gpu
        .retain_dynamic(&frame, &staged.pipelines)
        .unwrap();
    frame.finish().cancel();
}
