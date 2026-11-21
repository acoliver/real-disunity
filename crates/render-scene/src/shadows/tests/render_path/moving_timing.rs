use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use crate::device::DeviceRequest;
use crate::frame::FrameUniform;
use crate::{CameraPose, SceneView, Viewport};
use std::time::{Duration, Instant};
use worldrunner_render_post::{
    AttachmentView, FrameTiming, PassId, PassTiming, ResourceRef, TimingMode, TimingSupport,
};

const WARMUP: usize = 30;
const SAMPLES: usize = 120;
const WIDTH: u32 = 257;
const HEIGHT: u32 = 241;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Path {
    #[default]
    Forward,
    Render,
}

impl Path {
    fn scene_path(self) -> SceneOpaquePath {
        match self {
            Self::Forward => SceneOpaquePath::Forward,
            Self::Render => SceneOpaquePath::GBUFFER_RENDER,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Forward => 0,
            Self::Render => 1,
        }
    }
}

#[derive(Clone, Copy)]
struct PoseSample {
    view: SceneView,
    uniform: FrameUniform,
}

struct Resident {
    path: Path,
    gpu: crate::draw::SceneGpu,
    renderer: FrameRenderer,
    output: wgpu::Texture,
    timing: TimingSupport,
}

#[derive(Clone, Debug)]
struct Observation {
    view_index: usize,
    frame_id: u64,
    passes: Vec<worldrunner_render_post::PassSlot>,
    draws: crate::draw::PassDrawStatistics,
}

#[derive(Debug, Default)]
struct Measurements {
    path: Path,
    observations: Vec<Observation>,
    host_encode_submit: Vec<Duration>,
    completed_frame: Vec<Duration>,
    timings: Vec<FrameTiming>,
    expected_passes: Vec<PassId>,
    dropped_timing_frames: u64,
    unavailable_timing_passes: u64,
    timing_support: Option<TimingSupport>,
}

fn view_at(index: usize) -> SceneView {
    let eye_x = -400 + 800 * i64::try_from(index).unwrap() / 149;
    SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -4_000, 6_500],
            target_millimeters: [0, 0, 0],
        },
        *fixture::effects(600_000).camera(),
        Viewport {
            width: WIDTH,
            height: HEIGHT,
        },
    )
    .unwrap()
}

fn pose_sample(index: usize, source: &RenderScene) -> PoseSample {
    let view = view_at(index);
    let uniform = FrameUniform::new(&view, source.effects()).unwrap();
    PoseSample { view, uniform }
}

fn order(index: usize) -> [Path; 2] {
    if index.is_multiple_of(2) {
        [Path::Forward, Path::Render]
    } else {
        [Path::Render, Path::Forward]
    }
}

fn assert_moving_views(source: &RenderScene) {
    let first = pose_sample(0, source);
    let middle = pose_sample(74, source);
    let last = pose_sample(149, source);
    assert_ne!(first.view.eye_meters(), middle.view.eye_meters());
    assert_ne!(
        first.view.view_projection_matrix(),
        middle.view.view_projection_matrix()
    );
    assert_ne!(
        middle.view.view_projection_matrix(),
        last.view.view_projection_matrix()
    );
    assert_ne!(
        first.uniform.selection_view_projection(),
        middle.uniform.selection_view_projection()
    );
    let origin = glam::Vec3::ZERO;
    let authored_object = glam::Vec3::new(-1.5, 0.5, 0.3);
    let first_projection = first.view.view_projection_matrix().project_point3(origin);
    let middle_projection = middle.view.view_projection_matrix().project_point3(origin);
    let authored_first = first
        .view
        .view_projection_matrix()
        .project_point3(authored_object);
    let authored_middle = middle
        .view
        .view_projection_matrix()
        .project_point3(authored_object);
    assert_ne!(first_projection, middle_projection);
    assert_ne!(authored_first, authored_middle);
}

#[test]
fn moving_view_schedule_changes_pose_and_projection_and_alternates_paths() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let source = super::source(false, true);
    assert_moving_views(&source);
    assert_eq!(order(0), [Path::Forward, Path::Render]);
    assert_eq!(order(1), [Path::Render, Path::Forward]);
    assert_eq!(WARMUP + SAMPLES, 150);
}

#[test]
fn sampled_frame_ids_are_unique_and_timing_coverage_refuses_missing_pairs() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let ids: Vec<_> = (0..150)
        .flat_map(|index| order(index).map(|path| index * 2 + path.index() + 1))
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
    assert_eq!(ids.len(), 300);
    let timing = FrameTiming {
        frame_id: 19,
        passes: vec![PassTiming {
            pass_id: PassId::Resolve,
            duration_ns: Some(4.0),
        }],
    };
    assert_eq!(durations(&timing), Some(vec![(PassId::Resolve, 4.0)]));
    let mut missing = timing;
    missing.passes[0].duration_ns = None;
    assert_eq!(durations(&missing), None);
    let incomplete_pair = Measurements {
        timing_support: Some(TimingSupport::Supported),
        timings: vec![missing],
        ..Measurements::default()
    };
    assert_eq!(coverage(&incomplete_pair), (0, 1));
    assert!(!complete_summary(&incomplete_pair, 1));
    let missing_frame = Measurements {
        timing_support: Some(TimingSupport::Supported),
        ..Measurements::default()
    };
    assert!(!complete_summary(&missing_frame, 1));
    let values = [1.0, 2.0, 3.0];
    assert_eq!(median_p95(&values), Some((2.0, 3.0)));
}

#[test]
fn gpu_moving_view_smoke_draws_terrain_mesh_and_impostor_once() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = super::source(false, true);
    assert_moving_views(&source);
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let results = collect(&context, &source, WARMUP + 3);
    for (path, measurements) in [Path::Forward, Path::Render].into_iter().zip(&results) {
        assert_eq!(measurements.observations.len(), 3);
        for result in &measurements.observations {
            let (terrain, meshes, impostors) = draw_counts(path, &result.draws);
            assert!(terrain > 0, "{path:?} terrain draw stats: {result:?}");
            assert!(meshes > 0, "{path:?} mesh draw stats: {result:?}");
            assert!(impostors > 0, "{path:?} impostor draw stats: {result:?}");
        }
    }
    let passes: Vec<_> = results
        .iter()
        .flat_map(|result| &result.observations)
        .flat_map(|sample| sample.passes.iter().copied())
        .collect();
    assert_eq!(
        passes
            .iter()
            .filter(|pass| **pass == worldrunner_render_post::PassSlot::DepthNormalPrepass)
            .count(),
        6
    );
    assert_eq!(
        passes
            .iter()
            .filter(|pass| **pass == worldrunner_render_post::PassSlot::OpaqueGbuffer)
            .count(),
        0
    );
    assert_eq!(
        passes
            .iter()
            .filter(|pass| **pass == worldrunner_render_post::PassSlot::OpaqueLighting)
            .count(),
        3,
        "Render produces lighting once per sampled frame"
    );
}

fn timing_context() -> GpuContext {
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .unwrap();
    pollster::block_on(GpuContext::from_adapter(
        adapter,
        &DeviceRequest {
            features: wgpu::Features::empty(),
            timing: TimingMode::Optional,
            ..DeviceRequest::default()
        },
    ))
    .unwrap()
}

#[test]
#[ignore = "informational public moving-view timing probe; run explicitly with --ignored --nocapture"]
fn public_moving_view_timing_probe() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = timing_context();
    let source = super::source(false, true);
    assert_moving_views(&source);
    let results = collect(&context, &source, WARMUP + SAMPLES);
    report(&context, &results);
}

#[path = "moving_timing/collection.rs"]
mod collection;
#[path = "moving_timing/regressions.rs"]
mod regressions;
#[path = "moving_timing/reporting.rs"]
mod reporting;
#[path = "moving_timing/validation.rs"]
mod validation;
use collection::{collect, draw_counts, frame_id};
use reporting::{median_p95, report};
use validation::{complete_summary, coverage, durations};
