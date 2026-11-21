use super::*;
use crate::draw::PassDrawStatistics;
use worldrunner_render_post::{PassEntry, PassSlot, StageId};

struct FrameViews {
    color: AttachmentView,
    depth: AttachmentView,
    output: wgpu::TextureView,
}

impl FrameViews {
    fn new(resident: &Resident) -> Self {
        Self {
            color: AttachmentView::new(
                ResourceRef::new("scene color"),
                resident.renderer.targets().scene().texture(),
            )
            .unwrap(),
            depth: AttachmentView::new(
                ResourceRef::new("scene depth"),
                resident.renderer.targets().depth().texture(),
            )
            .unwrap(),
            output: resident
                .output
                .create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

fn resident(
    context: &GpuContext,
    source: &RenderScene,
    pipelines: &ScenePipelines,
    path: Path,
) -> Resident {
    let renderer = deferred::frame(context, source, &view_at(0), path.scene_path());
    let timing = renderer
        .configure_timing(context.device(), context.queue(), TimingMode::Optional)
        .unwrap();
    Resident {
        path,
        gpu: crate::draw::SceneGpu::upload(
            context.device(),
            context.submission(),
            source,
            pipelines,
        )
        .unwrap(),
        renderer,
        output: worldrunner_render_post::color_texture(
            context.device(),
            "moving view timing output",
            [WIDTH, HEIGHT],
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ),
        timing,
    }
}

fn expected_passes(resident: &Resident) -> Vec<PassId> {
    // This fixture has metadata, no water/reflections/celestials, and single-sample HDR
    // output. Read the installed screen schedule, independently of timestamp records.
    let screen = resident.renderer.ssao.as_ref().expect("fixture metadata");
    let mut entries = vec![PassEntry::new(PassSlot::DepthNormalPrepass)];
    entries.extend(
        screen
            .passes
            .schedule_shape()
            .into_iter()
            .map(|(slot, _)| PassEntry::new(slot)),
    );
    if resident.path == Path::Render {
        entries.push(PassEntry::new(PassSlot::OpaqueLighting));
    }
    entries.push(PassEntry::new(PassSlot::Opaque));
    let mut ids: Vec<_> = entries.into_iter().map(PassId::Geometry).collect();
    // Single-sample FrameTargets has no MSAA resolve. The fixed Reinhard output
    // schedules tone level zero (render-post/tone_map.rs).
    ids.push(PassId::Stage(StageId::new("tone", 0)));
    ids
}

pub(super) fn frame_id(index: usize, path: Path) -> u64 {
    u64::try_from(index * 2 + path.index() + 1).unwrap()
}

fn submit_frame(
    context: &GpuContext,
    pipelines: &ScenePipelines,
    resident: &mut Resident,
    views: &FrameViews,
    uniform: &FrameUniform,
    frame_id: u64,
) -> wgpu::SubmissionIndex {
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            if resident.timing == TimingSupport::Supported {
                resident.renderer.begin_timing(frame_id).unwrap();
            }
            encode_scene(
                &mut resident.renderer,
                context.device(),
                encoder,
                (&mut resident.gpu, pipelines),
                (&views.color, &views.depth),
                *uniform,
            )
            .unwrap();
            resident
                .renderer
                .encode_output(encoder, &views.output)
                .unwrap();
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap()
        .submit()
        .unwrap()
}

fn run_frame(
    context: &GpuContext,
    pipelines: &ScenePipelines,
    resident: &mut Resident,
    sample: &PoseSample,
    frame_id: u64,
) -> (Duration, Duration) {
    let views = FrameViews::new(resident);
    let start = Instant::now();
    let submission = submit_frame(
        context,
        pipelines,
        resident,
        &views,
        &sample.uniform,
        frame_id,
    );
    let encode_submit = start.elapsed();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    (encode_submit, start.elapsed())
}

fn take_frame(resident: &mut Resident, device: &wgpu::Device) -> Option<FrameTiming> {
    if resident.timing != TimingSupport::Supported {
        return None;
    }
    let mut frames =
        crate::test_support::take_timings(device, 1, || resident.renderer.take_timings());
    assert_eq!(frames.len(), 1, "one timing record for completed frame");
    Some(frames.remove(0))
}

pub(super) fn draw_counts(path: Path, draws: &PassDrawStatistics) -> (u64, u64, u64) {
    let objects = match path {
        Path::Forward => draws.opaque_objects,
        Path::Render => draws.prepass_objects,
    };
    (
        draws.terrain.calls,
        objects.meshes.calls,
        objects.impostors.calls,
    )
}

fn assert_observation(path: Path, sample: &Observation) {
    let count = |slot| sample.passes.iter().filter(|pass| **pass == slot).count();
    assert_eq!(
        count(PassSlot::DepthNormalPrepass),
        1,
        "{path:?}: {sample:?}"
    );
    assert_eq!(count(PassSlot::OpaqueGbuffer), 0, "{path:?}: {sample:?}");
    assert_eq!(
        count(PassSlot::OpaqueLighting),
        usize::from(path == Path::Render)
    );
    let (terrain, meshes, impostors) = draw_counts(path, &sample.draws);
    assert!(
        terrain > 0 && meshes > 0 && impostors > 0,
        "{path:?}: {sample:?}"
    );
    if path == Path::Render {
        let opaque = sample.draws.opaque_objects;
        assert_eq!(opaque.meshes.calls, 0);
        assert_eq!(opaque.meshes.instances, 0);
        assert_eq!(opaque.impostors.calls, 0);
        assert_eq!(opaque.impostors.instances, 0);
    }
}

fn record_sample(
    target: &Resident,
    result: &mut Measurements,
    observation: Observation,
    elapsed: (Duration, Duration),
    timing: Option<FrameTiming>,
) {
    if observation.view_index < WARMUP {
        return;
    }
    assert_observation(target.path, &observation);
    result.host_encode_submit.push(elapsed.0);
    result.completed_frame.push(elapsed.1);
    result.observations.push(observation);
    if let Some(timing) = timing {
        result.timings.push(timing);
    }
    result.dropped_timing_frames = target.renderer.dropped_timing_frames();
    result.unavailable_timing_passes = target.renderer.unavailable_timing_passes();
}

fn collect_frame(
    context: &GpuContext,
    pipelines: &ScenePipelines,
    target: &mut Resident,
    result: &mut Measurements,
    sample: &PoseSample,
    view_index: usize,
) {
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    let id = frame_id(view_index, target.path);
    let elapsed = run_frame(context, pipelines, target, sample, id);
    let observation = Observation {
        view_index,
        frame_id: id,
        passes: crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(std::mem::take),
        draws: target.gpu.statistics().passes,
    };
    // Plan tracing belongs to other behavioral tests; do not accumulate its copies here.
    crate::compositor::plan_trace::take();
    let timing = take_frame(target, context.device());
    if let Some(timing) = &timing {
        assert_eq!(timing.frame_id, id);
    }
    record_sample(target, result, observation, elapsed, timing);
}

pub(super) fn collect(
    context: &GpuContext,
    source: &RenderScene,
    count: usize,
) -> [Measurements; 2] {
    let pipelines =
        ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let mut residents =
        [Path::Forward, Path::Render].map(|path| resident(context, source, &pipelines, path));
    let mut results = std::array::from_fn(|index| Measurements {
        path: residents[index].path,
        timing_support: Some(residents[index].timing),
        expected_passes: expected_passes(&residents[index]),
        ..Measurements::default()
    });
    for index in 0..count {
        let sample = pose_sample(index, source);
        for path in order(index) {
            collect_frame(
                context,
                &pipelines,
                &mut residents[path.index()],
                &mut results[path.index()],
                &sample,
                index,
            );
        }
    }
    assert!(results.iter().all(|result| {
        result.host_encode_submit.len() == count.saturating_sub(WARMUP)
            && result.observations.len() == count.saturating_sub(WARMUP)
    }));
    results
}
