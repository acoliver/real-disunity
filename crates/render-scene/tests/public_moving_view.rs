#[path = "public_moving_view/collection.rs"]
mod collection;
#[path = "public_moving_view/fixture.rs"]
mod fixture;
#[path = "public_moving_view/validation.rs"]
mod validation;

use collection::{Measurements, Path, SAMPLES, WARMUP, collect, expected, frame_id, order};
use std::time::Duration;
use validation::{duration_values, median_p95, valid_measurements};
use worldrunner_render_gpu::device::headless_instance;
use worldrunner_render_post::{TimingMode, TimingSupport};
use worldrunner_render_scene::{SceneView, device::GpuContext, wgpu};

fn timing_context() -> GpuContext {
    let instance = headless_instance();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect("public renderer probe requires a real GPU adapter");
    pollster::block_on(GpuContext::from_adapter_with_timing(
        adapter,
        wgpu::Features::empty(),
        TimingMode::Optional,
    ))
    .expect("public renderer probe device acquisition")
}

fn view_schedule(index: usize) -> SceneView {
    fixture::view(index)
}

fn assert_pose_schedule() {
    let source = fixture::source();
    let first = view_schedule(0);
    let middle = view_schedule(74);
    let last = view_schedule(149);
    for index in 0..150 {
        let eye_x = -400 + 800 * i64::try_from(index).unwrap() / 149;
        let expected = SceneView::new(
            worldrunner_render_scene::CameraPose {
                eye_millimeters: [eye_x, -4_000, 6_500],
                target_millimeters: [0, 0, 0],
            },
            *source.effects().unwrap().camera(),
            worldrunner_render_scene::Viewport {
                width: 257,
                height: 241,
            },
        )
        .unwrap();
        assert_eq!(
            view_schedule(index).view_projection_matrix(),
            expected.view_projection_matrix()
        );
    }
    assert_eq!(first.eye_meters().x.to_bits(), (-0.4_f32).to_bits());
    assert_eq!(last.eye_meters().x.to_bits(), 0.4_f32.to_bits());
    assert_ne!(
        first.view_projection_matrix(),
        middle.view_projection_matrix()
    );
    assert_ne!(
        middle.view_projection_matrix(),
        last.view_projection_matrix()
    );
    assert_eq!(source.terrain().unwrap().columns(), 7);
    assert_eq!(source.geometry().objects().len(), 2);
    assert_eq!(order(0), [Path::Forward, Path::Render]);
    assert_eq!(order(1), [Path::Render, Path::Forward]);
    assert_eq!(order(148), [Path::Forward, Path::Render]);
    assert_eq!(order(149), [Path::Render, Path::Forward]);
    let ids: Vec<_> = (0..WARMUP + SAMPLES)
        .flat_map(|index| order(index).map(|path| frame_id(index, path)))
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), 300);
    assert_eq!(unique.len(), 300);
}

#[test]
fn public_schedule_has_exact_pose_path_and_frame_id_coverage() {
    assert_pose_schedule();
    assert_eq!(expected(Path::Forward).len(), 15);
    assert_eq!(expected(Path::Render).len(), 16);
}

fn smoke_measurements(results: &[Measurements; 2]) {
    for (path, result) in [Path::Forward, Path::Render].into_iter().zip(results) {
        assert_eq!(result.path, Some(path));
        assert_eq!(result.observations.len(), 2);
        assert!(
            valid_measurements(result, 2),
            "{path:?}: incomplete smoke records"
        );
    }
}

#[test]
fn gpu_public_moving_view_smoke_draws_fixture_objects_through_both_paths() {
    let context = timing_context();
    let results = collect(&context, &fixture::source(), WARMUP + 2);
    smoke_measurements(&results);
}

#[test]
#[ignore = "informational public moving-view renderer measurement; run explicitly with --ignored --nocapture"]
fn public_moving_view_renderer_timing_probe() {
    assert_pose_schedule();
    let context = timing_context();
    let results = collect(&context, &fixture::source(), WARMUP + SAMPLES);
    report(&context, &results);
}

fn durations_ms(values: &[Duration]) -> Option<(f64, f64)> {
    median_p95(
        &values
            .iter()
            .map(|duration| duration.as_secs_f64() * 1_000.0)
            .collect::<Vec<_>>(),
    )
}

fn report(context: &GpuContext, results: &[Measurements; 2]) {
    let info = context.adapter().get_info();
    println!(
        "adapter={} vendor={} device={} driver={} backend={:?} type={:?} identity={:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver,
        info.backend,
        info.device_type,
        context.adapter_identity(),
    );
    println!(
        "adapter_features={:?} enabled_features={:?} adapter_limits={:?} device_limits={:?} timestamp_period_ns={} debug_assertions={}",
        context.adapter().features(),
        context.device().features(),
        context.adapter().limits(),
        context.device().limits(),
        context.queue().get_timestamp_period(),
        cfg!(debug_assertions),
    );
    println!(
        "synthetic fixture only; 257x241 eye_x_mm=-400+800*i/149 target=[0,0,0], warmup={WARMUP}, sampled={SAMPLES}, alternating Forward/GBUFFER_RENDER; timestamps are GPU sums of timed passes, not frame envelopes or throughput"
    );
    println!(
        "CPU encode/submit includes submission and ends when submit returns; serialized completion is the per-frame wait from encode start. Timestamp resolution and observations occur after that interval."
    );
    for result in results {
        let path = result.path.unwrap();
        assert_eq!(result.observations.len(), SAMPLES);
        assert!(
            valid_measurements(result, SAMPLES),
            "{path:?}: sampled coverage invalid"
        );
        println!(
            "{path:?}: frame associations {}/{}; CPU encode/submit median/p95 ms {:?}; serialized completion median/p95 ms {:?}; timing support={:?} dropped={} unavailable={}",
            result.observations.len(),
            SAMPLES,
            durations_ms(&result.encode_submit),
            durations_ms(&result.completion),
            result.support,
            result.dropped,
            result.unavailable,
        );
        report_gpu(result);
    }
}

fn report_gpu(result: &Measurements) {
    let path = result.path.unwrap();
    if result.support != Some(TimingSupport::Supported) {
        println!(
            "{path:?}: GPU timing coverage 0/{SAMPLES}; records=0; timestamps unavailable; CPU-only measurements"
        );
        return;
    }
    assert!(
        valid_measurements(result, SAMPLES),
        "supported GPU timing has missing or invalid records"
    );
    println!(
        "{path:?}: GPU timing coverage {SAMPLES}/{SAMPLES}; received {}/{}; dropped={} unavailable={}",
        result.timings.len(),
        SAMPLES,
        result.dropped,
        result.unavailable,
    );
    for id in &result.expected {
        let values: Vec<_> = result
            .timings
            .iter()
            .map(|timing| {
                let entries = duration_values(timing, &result.expected).expect("validated frame");
                let index = timing
                    .passes
                    .iter()
                    .position(|pass| pass.pass_id == *id)
                    .expect("validated pass identity");
                entries[index]
            })
            .collect();
        println!("  GPU {id:?} median/p95 ns {:?}", median_p95(&values));
    }
    let sums: Vec<_> = result
        .timings
        .iter()
        .map(|timing| {
            duration_values(timing, &result.expected)
                .expect("validated frame")
                .iter()
                .sum()
        })
        .collect();
    println!(
        "  GPU timed-pass sum median/p95 ns {:?}; sum excludes gaps and untimed work, not a frame envelope",
        median_p95(&sums)
    );
}
