use super::*;

pub(super) fn durations(timing: &FrameTiming) -> Option<Vec<(PassId, f64)>> {
    if timing.passes.is_empty() {
        return None;
    }
    let mut result = Vec::with_capacity(timing.passes.len());
    for PassTiming {
        pass_id,
        duration_ns,
    } in &timing.passes
    {
        let ns = (*duration_ns)?;
        if !ns.is_finite() || ns < 0.0 {
            return None;
        }
        result.push((*pass_id, ns));
    }
    Some(result)
}

fn valid_passes(timing: &FrameTiming, expected: &[PassId]) -> bool {
    let Some(entries) = durations(timing) else {
        return false;
    };
    if expected.is_empty() || entries.len() != expected.len() {
        return false;
    }
    let mut remaining = expected.to_vec();
    for (id, _) in entries {
        let Some(index) = remaining.iter().position(|pass| *pass == id) else {
            return false;
        };
        remaining.remove(index);
    }
    remaining.is_empty()
}

fn valid_sample(result: &Measurements, index: usize, timing: &FrameTiming) -> bool {
    result.observations.get(index).is_some_and(|sample| {
        sample.view_index == WARMUP + index
            && sample.frame_id == frame_id(WARMUP + index, result.path)
            && timing.frame_id == sample.frame_id
    })
}

pub(super) fn coverage(measurements: &Measurements) -> (usize, usize) {
    let complete = measurements
        .timings
        .iter()
        .enumerate()
        .filter(|(index, timing)| {
            valid_sample(measurements, *index, timing)
                && valid_passes(timing, &measurements.expected_passes)
        })
        .count();
    (complete, measurements.timings.len())
}

pub(super) fn complete_summary(measurements: &Measurements, expected: usize) -> bool {
    let (complete, received) = coverage(measurements);
    expected == SAMPLES
        && measurements.timing_support == Some(TimingSupport::Supported)
        && complete == expected
        && received == expected
        && measurements.observations.len() == expected
        && measurements.dropped_timing_frames == 0
        && measurements.unavailable_timing_passes == 0
}
