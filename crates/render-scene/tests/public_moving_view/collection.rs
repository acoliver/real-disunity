use super::*;
use std::time::{Duration, Instant};
use worldrunner_render_plan::{RenderScene, SceneOpaquePath};
use worldrunner_render_post::{
    AttachmentView, FrameTiming, PassEntry, PassId, PassSlot, ResourceRef, StageId, TimingMode,
    TimingSupport, ToneMap,
};
use worldrunner_render_scene::{
    SceneView,
    compositor::{FrameRenderer, encode_scene},
    device::GpuContext,
    draw::{ObjectDraws, PassDrawStatistics, SceneGpu},
    executor,
    frame::FrameUniform,
    offscreen::OffscreenTarget,
    renderer::ScenePipelines,
    wgpu,
};

pub const WARMUP: usize = 30;
pub const SAMPLES: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Forward,
    Render,
}
impl Path {
    pub fn index(self) -> usize {
        match self {
            Self::Forward => 0,
            Self::Render => 1,
        }
    }
    fn opaque_path(self) -> SceneOpaquePath {
        match self {
            Self::Forward => SceneOpaquePath::Forward,
            Self::Render => SceneOpaquePath::GBUFFER_RENDER,
        }
    }
}

#[derive(Debug)]
pub struct Observation {
    pub view_index: usize,
    pub frame_id: u64,
    pub draws: PassDrawStatistics,
}

#[derive(Debug, Default)]
pub struct Measurements {
    pub path: Option<Path>,
    pub observations: Vec<Observation>,
    pub encode_submit: Vec<Duration>,
    pub completion: Vec<Duration>,
    pub timings: Vec<FrameTiming>,
    pub expected: Vec<PassId>,
    pub support: Option<TimingSupport>,
    pub dropped: u64,
    pub unavailable: u64,
}

struct Resident {
    path: Path,
    gpu: SceneGpu,
    renderer: FrameRenderer,
    effects: worldrunner_render_plan::SceneEffects,
    output: wgpu::Texture,
    support: TimingSupport,
}

pub fn frame_id(index: usize, path: Path) -> u64 {
    u64::try_from(index * 2 + path.index() + 1).unwrap()
}

pub fn order(index: usize) -> [Path; 2] {
    if index.is_multiple_of(2) {
        [Path::Forward, Path::Render]
    } else {
        [Path::Render, Path::Forward]
    }
}

pub fn expected(path: Path) -> Vec<PassId> {
    let mut passes = vec![PassId::Geometry(PassEntry::new(
        PassSlot::DepthNormalPrepass,
    ))];
    passes.extend(
        (0..10).map(|level| PassId::Geometry(PassEntry::new(PassSlot::DepthPyramid(level)))),
    );
    passes.extend([
        PassId::Geometry(PassEntry::new(PassSlot::Ssao)),
        PassId::Geometry(PassEntry::new(PassSlot::SsaoBlur)),
    ]);
    if path == Path::Render {
        passes.push(PassId::Geometry(PassEntry::new(PassSlot::OpaqueLighting)));
    }
    passes.push(PassId::Geometry(PassEntry::new(PassSlot::Opaque)));
    passes.push(PassId::Stage(StageId::new("tone", 0)));
    passes
}

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
    let effects = source
        .effects()
        .unwrap()
        .clone()
        .with_opaque_path(path.opaque_path())
        .unwrap();
    let renderer = FrameRenderer::hdr(
        context,
        fixture::SIZE,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap();
    let support = renderer
        .configure_timing(context.device(), context.queue(), TimingMode::Optional)
        .unwrap();
    Resident {
        path,
        gpu: SceneGpu::upload(context.device(), context.submission(), source, pipelines).unwrap(),
        renderer,
        effects,
        output: executor::color_texture(
            context.device(),
            "public moving-view output",
            fixture::SIZE,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ),
        support,
    }
}

fn submit(
    context: &GpuContext,
    pipelines: &ScenePipelines,
    resident: &mut Resident,
    views: &FrameViews,
    uniform: &FrameUniform,
    id: u64,
) -> wgpu::SubmissionIndex {
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            if resident.support == TimingSupport::Supported {
                resident.renderer.begin_timing(id).unwrap();
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

fn collect_one(
    context: &GpuContext,
    pipelines: &ScenePipelines,
    resident: &mut Resident,
    view: &SceneView,
    index: usize,
) -> (Observation, Duration, Duration, Option<FrameTiming>) {
    let views = FrameViews::new(resident);
    let uniform = FrameUniform::new(view, Some(&resident.effects)).unwrap();
    let id = frame_id(index, resident.path);
    let start = Instant::now();
    let submission = submit(context, pipelines, resident, &views, &uniform, id);
    let encode_submit = start.elapsed();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    let completion = start.elapsed();
    let draws = resident.gpu.statistics().passes;
    let timing = read_timing(resident, context.device(), id);
    (
        Observation {
            view_index: index,
            frame_id: id,
            draws,
        },
        encode_submit,
        completion,
        timing,
    )
}

fn read_timing(resident: &mut Resident, device: &wgpu::Device, id: u64) -> Option<FrameTiming> {
    if resident.support != TimingSupport::Supported {
        return None;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut frames = resident.renderer.take_timings().unwrap();
        if !frames.is_empty() {
            assert_eq!(
                frames.len(),
                1,
                "one timing record after its frame completed"
            );
            let frame = frames.remove(0);
            assert_eq!(frame.frame_id, id);
            return Some(frame);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "timestamp resolution timed out");
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(remaining),
            })
            .unwrap();
    }
}

fn new_measurements(resident: &Resident) -> Measurements {
    Measurements {
        path: Some(resident.path),
        expected: expected(resident.path),
        support: Some(resident.support),
        ..Measurements::default()
    }
}

fn record(
    result: &mut Measurements,
    sample: (Observation, Duration, Duration, Option<FrameTiming>),
    index: usize,
    resident: &Resident,
) {
    if index < WARMUP {
        return;
    }
    validate_draws(resident.path, &sample.0.draws);
    result.observations.push(sample.0);
    result.encode_submit.push(sample.1);
    result.completion.push(sample.2);
    if let Some(timing) = sample.3 {
        result.timings.push(timing);
    }
    result.dropped = resident.renderer.dropped_timing_frames();
    result.unavailable = resident.renderer.unavailable_timing_passes();
}

fn validate_draws(path: Path, draws: &PassDrawStatistics) {
    let objects = if path == Path::Forward {
        draws.opaque_objects
    } else {
        draws.prepass_objects
    };
    assert_eq!(draws.terrain.calls, 1, "terrain draw count");
    assert_eq!(objects.meshes.calls, 1, "mesh draw count");
    assert_eq!(objects.impostors.calls, 1, "impostor draw count");
    if path == Path::Render {
        assert!(draws.prepass_objects.meshes.calls > 0);
        assert_eq!(draws.opaque_objects, ObjectDraws::default());
    }
}

pub fn collect(context: &GpuContext, source: &RenderScene, count: usize) -> [Measurements; 2] {
    let pipelines =
        ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let mut residents =
        [Path::Forward, Path::Render].map(|path| resident(context, source, &pipelines, path));
    let mut results = residents.each_ref().map(new_measurements);
    for index in 0..count {
        let view = fixture::view(index);
        for path in order(index) {
            let slot = path.index();
            let sample = collect_one(context, &pipelines, &mut residents[slot], &view, index);
            record(&mut results[slot], sample, index, &residents[slot]);
        }
    }
    results
}
