//! Informational probe. Run serially with `--ignored --nocapture`.
use super::*;
use worldrunner_render_post::{FrameTiming, PassId, TimingMode};

const WARMUP_FRAMES: u64 = 8;
const SAMPLE_FRAMES: u64 = 32;

struct Samples {
    caster_ns: Vec<f64>,
    total_ns: Vec<f64>,
    refreshed_layers: Vec<usize>,
}

#[test]
#[ignore = "explicit informational cascade refresh GPU timing probe"]
fn cascade_refresh_gpu_timing_comparison() {
    let context = match pollster::block_on(GpuContext::request_with_features(
        wgpu::Features::TIMESTAMP_QUERY,
    )) {
        Ok(context) => context,
        Err(crate::device::DeviceError::MissingFeatures { missing, .. })
            if missing.contains(wgpu::Features::TIMESTAMP_QUERY) =>
        {
            eprintln!("SKIP cascade refresh timing: adapter lacks TIMESTAMP_QUERY");
            return;
        }
        Err(error) => panic!("cascade refresh timing GPU acquisition failed: {error}"),
    };
    let casters: Vec<_> = (0..1_024)
        .map(|i| [(i % 32) * 150 - 2_325, (i / 32) * 150 - 2_325, 1_000])
        .collect();
    let source = cascades::source_with(
        &scene(&casters, 600_000, None),
        SceneShadowCascades::new(vec![1_000, 2_000, 3_000, 4_000], 0).unwrap(),
    );
    let default = measure(&context, &source, CascadeRefreshPolicy::default());
    let staggered = measure(
        &context,
        &source,
        CascadeRefreshPolicy {
            intervals: [1, 2, 4, 4],
            phases: [0, 0, 1, 3],
            max_non_near: 1,
            light_delta_radians: 0.01,
            projection_delta_texels: 2.0,
        },
    );
    print_comparison(&context, &default, &staggered);
    assert!(default.refreshed_layers.iter().all(|&count| count == 4));
    assert!(staggered.refreshed_layers.iter().all(|&count| count == 2));
    assert!(
        staggered.refreshed_layers.iter().sum::<usize>()
            < default.refreshed_layers.iter().sum::<usize>()
    );
}

fn measure(context: &GpuContext, source: &RenderScene, policy: CascadeRefreshPolicy) -> Samples {
    let mut renderer = ShadowedScene::new(
        context,
        source,
        DirectionalShadowSettings {
            refresh_policy: policy,
            ..settings()
        },
    )
    .unwrap();
    renderer
        .scheduler
        .configure_timing(context.device(), context.queue(), TimingMode::Required)
        .unwrap();
    let view = view();
    let target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let mut samples = Samples {
        caster_ns: Vec::new(),
        total_ns: Vec::new(),
        refreshed_layers: Vec::new(),
    };
    for frame in 0..WARMUP_FRAMES + SAMPLE_FRAMES {
        let timing = timed_frame(&mut renderer, context, &view, &target, frame);
        if frame < WARMUP_FRAMES {
            continue;
        }
        let (caster, total) = durations(&timing);
        samples.caster_ns.push(caster);
        samples.total_ns.push(total);
        samples.refreshed_layers.push(
            renderer
                .cascade_refresh_states()
                .iter()
                .filter(|state| state.last_frame() == Some(frame))
                .count(),
        );
    }
    samples
}

fn timed_frame(
    renderer: &mut ShadowedScene,
    context: &GpuContext,
    view: &SceneView,
    target: &OffscreenTarget,
    frame: u64,
) -> FrameTiming {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer.scheduler.begin_timing(frame).unwrap();
    renderer
        .encode(
            context,
            view,
            &target.color_attachment(),
            &target.depth_attachment(),
            &mut encoder,
        )
        .unwrap();
    renderer.scheduler.finish_timing(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let mut timings = renderer.scheduler.take_timings().unwrap();
    let timing = timings.pop().expect("required frame timestamp result");
    assert_eq!(timing.frame_id, frame);
    timing
}

fn durations(timing: &FrameTiming) -> (f64, f64) {
    let mut caster = 0.0;
    let mut total = 0.0;
    for pass in &timing.passes {
        let ns = pass.duration_ns.expect("unavailable GPU timestamp pair");
        total += ns;
        if matches!(pass.pass_id, PassId::Geometry(PassSlot::ShadowCasters(_))) {
            caster += ns;
        }
    }
    (caster, total)
}

fn statistics_ns(samples: &[f64]) -> (f64, f64) {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = (sorted[15] + sorted[16]) * 0.5;
    let mean = sorted.iter().sum::<f64>() / f64::from(u32::try_from(sorted.len()).unwrap());
    (median, mean)
}

fn print_comparison(context: &GpuContext, default: &Samples, staggered: &Samples) {
    eprintln!(
        "Cascade refresh timing: {}",
        context.adapter().get_info().name
    );
    eprintln!("Static camera/light, 1024 casters, 4 x 512px maps, 257 x 241 target");
    eprintln!(
        "Staggered: intervals [1,2,4,4], phases [0,0,1,3], budget 1, light 0.01rad, XY 2texels"
    );
    eprintln!("Per policy: 8 warmup frames, 32 sampled frames; GPU times in ms/frame");
    eprintln!("Caster = sum across cascades; total = sum of all timed passes (excludes gaps)");
    eprintln!(
        "policy    | caster median | caster mean | total median | total mean | layers total | layers/frame"
    );
    for (name, samples) in [("default", default), ("staggered", staggered)] {
        let (caster_median, caster_mean) = statistics_ns(&samples.caster_ns);
        let (total_median, total_mean) = statistics_ns(&samples.total_ns);
        eprintln!(
            "{name:9} | {:13.3} | {:11.3} | {:12.3} | {:10.3} | {:12} | {:?}",
            caster_median / 1_000_000.0,
            caster_mean / 1_000_000.0,
            total_median / 1_000_000.0,
            total_mean / 1_000_000.0,
            samples.refreshed_layers.iter().sum::<usize>(),
            samples.refreshed_layers,
        );
    }
}
