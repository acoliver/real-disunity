use super::*;

pub(super) fn median_p95(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let p95 = sorted[(sorted.len() * 95).div_ceil(100).saturating_sub(1)];
    Some((median, p95))
}

fn duration_stats(values: &[Duration]) -> Option<(f64, f64)> {
    median_p95(
        &values
            .iter()
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .collect::<Vec<_>>(),
    )
}

fn report_adapter(context: &GpuContext, results: &[Measurements; 2]) {
    let info = context.adapter().get_info();
    println!(
        "moving-view, unshadowed renderer timing: adapter={} vendor={} device={} driver={} backend={:?} software={} identity={:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver,
        info.backend,
        info.device_type == wgpu::DeviceType::Cpu,
        context.adapter_identity(),
    );
    println!(
        "enabled_features={:?} limits={:?} timestamp_period_ns={} negotiated_timestamp_support={:?}; synthetic source revision unavailable",
        context.device().features(),
        context.device().limits(),
        context.queue().get_timestamp_period(),
        results
            .each_ref()
            .map(|result| (result.path, result.timing_support)),
    );
}

pub(super) fn report(context: &GpuContext, results: &[Measurements; 2]) {
    report_adapter(context, results);
    println!(
        "view=257x241 eye_x_mm=-400..400 target=[0,0,0] warmup={WARMUP} sampled={SAMPLES}; Forward vs GBUFFER_RENDER; no shadows/SDSM; each renderer resident across every frame"
    );
    println!(
        "CPU encode/submit ends at submission; serialized completion includes per-frame wait and excludes later timestamp resolve and observations. Neither is GPU time or overlapped throughput."
    );
    for result in results {
        println!(
            "{:?}: CPU encode/submit median/p95 {:?} ms; serialized completion median/p95 {:?} ms; submitted draw-call ranges terrain/mesh/impostor {:?}; sampled observations={}",
            result.path,
            duration_stats(&result.host_encode_submit),
            duration_stats(&result.completed_frame),
            draw_ranges(result),
            result.observations.len(),
        );
        print_gpu(result);
    }
}

fn draw_ranges(result: &Measurements) -> [(u64, u64); 3] {
    std::array::from_fn(|index| {
        let values: Vec<_> = result
            .observations
            .iter()
            .map(|sample| {
                let (terrain, meshes, impostors) = draw_counts(result.path, &sample.draws);
                [terrain, meshes, impostors][index]
            })
            .collect();
        (
            values.iter().copied().min().unwrap_or(0),
            values.iter().copied().max().unwrap_or(0),
        )
    })
}

fn print_gpu(result: &Measurements) {
    let path = result.path;
    let (complete, received) = coverage(result);
    println!(
        "{path:?}: negotiated timestamp support {:?}; complete GPU frame coverage {complete}/{SAMPLES}, received {received}/{SAMPLES}; dropped_timing_frames={} unavailable_timing_passes={}",
        result.timing_support, result.dropped_timing_frames, result.unavailable_timing_passes,
    );
    if result.timing_support != Some(TimingSupport::Supported) {
        println!("{path:?}: GPU timestamps unavailable; CPU-only run");
        return;
    }
    if !complete_summary(result, SAMPLES) {
        println!("{path:?}: incomplete GPU summary withheld");
        return;
    }
    for id in &result.expected_passes {
        let samples: Vec<_> = result
            .timings
            .iter()
            .map(|timing| {
                timing
                    .passes
                    .iter()
                    .find(|pass| pass.pass_id == *id)
                    .expect("identity coverage checked")
                    .duration_ns
                    .expect("duration checked")
            })
            .collect();
        println!("  GPU {id:?}: median/p95 ns {:?}", median_p95(&samples));
    }
    let sums: Vec<_> = result
        .timings
        .iter()
        .map(|timing| {
            durations(timing)
                .expect("coverage checked")
                .iter()
                .map(|(_, ns)| ns)
                .sum()
        })
        .collect();
    println!(
        "  GPU sum of timed passes median/p95 ns {:?}; gaps and untimed work excluded, not a frame envelope",
        median_p95(&sums)
    );
}
