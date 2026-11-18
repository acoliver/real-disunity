use super::*;
use worldrunner_render_post::{FrameTiming, PassId, PassTiming, TimingSupport};

pub fn duration_values(frame: &FrameTiming, expected: &[PassId]) -> Option<Vec<f64>> {
    if frame.passes.len() != expected.len() {
        return None;
    }
    let mut remaining = expected.to_vec();
    let mut values = Vec::with_capacity(expected.len());
    for PassTiming {
        pass_id,
        duration_ns,
    } in &frame.passes
    {
        let position = remaining.iter().position(|expected| expected == pass_id)?;
        remaining.remove(position);
        let duration = (*duration_ns)?;
        if !duration.is_finite() || duration < 0.0 {
            return None;
        }
        values.push(duration);
    }
    remaining.is_empty().then_some(values)
}

pub fn valid_measurements(result: &Measurements, expected_samples: usize) -> bool {
    let (Some(path), Some(support)) = (result.path, result.support) else {
        return false;
    };
    if result.expected != expected(path)
        || result.observations.len() != expected_samples
        || result.encode_submit.len() != expected_samples
        || result.completion.len() != expected_samples
    {
        return false;
    }
    if result
        .observations
        .iter()
        .enumerate()
        .any(|(offset, observation)| {
            observation.view_index != WARMUP + offset
                || observation.frame_id != frame_id(observation.view_index, path)
        })
    {
        return false;
    }
    if support != TimingSupport::Supported {
        return result.timings.is_empty() && result.dropped == 0 && result.unavailable == 0;
    }
    result.dropped == 0
        && result.unavailable == 0
        && result.timings.len() == expected_samples
        && result.timings.iter().enumerate().all(|(offset, frame)| {
            frame.frame_id == result.observations[offset].frame_id
                && duration_values(frame, &result.expected).is_some()
        })
}

pub fn median_p95(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some((
        sorted[sorted.len() / 2],
        sorted[(sorted.len() * 95).div_ceil(100) - 1],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_post::{PassEntry, PassSlot, StageId};
    use worldrunner_render_scene::draw::PassDrawStatistics;

    fn synthetic() -> (FrameTiming, Vec<PassId>) {
        let ids = vec![
            PassId::Geometry(PassEntry::new(PassSlot::DepthNormalPrepass)),
            PassId::Geometry(PassEntry::new(PassSlot::Opaque)),
            PassId::Stage(StageId::new("tone", 0)),
        ];
        let timing = FrameTiming {
            frame_id: 81,
            passes: ids
                .iter()
                .enumerate()
                .map(|(index, pass_id)| PassTiming {
                    pass_id: *pass_id,
                    duration_ns: Some(f64::from(u32::try_from(index).unwrap())),
                })
                .collect(),
        };
        (timing, ids)
    }

    fn sampled_measurements() -> Measurements {
        let expected = expected(Path::Forward);
        let timing = FrameTiming {
            frame_id: 61,
            passes: expected
                .iter()
                .map(|pass_id| PassTiming {
                    pass_id: *pass_id,
                    duration_ns: Some(0.0),
                })
                .collect(),
        };
        Measurements {
            path: Some(Path::Forward),
            observations: vec![collection::Observation {
                view_index: 30,
                frame_id: 61,
                draws: PassDrawStatistics::default(),
            }],
            encode_submit: vec![Duration::ZERO],
            completion: vec![Duration::ZERO],
            timings: vec![timing],
            expected,
            support: Some(TimingSupport::Supported),
            dropped: 0,
            unavailable: 0,
        }
    }

    #[test]
    fn sampled_validation_requires_complete_host_and_gpu_frame_associations() {
        assert!(valid_measurements(&sampled_measurements(), 1));
        let mutations: [fn(&mut Measurements); 11] = [
            |result| result.path = Some(Path::Render),
            |result| result.observations[0].view_index = 31,
            |result| result.observations[0].frame_id = 62,
            |result| result.timings[0].frame_id = 62,
            |result| result.observations.clear(),
            |result| result.timings.clear(),
            |result| {
                result.expected.pop();
            },
            |result| result.encode_submit.clear(),
            |result| result.completion.clear(),
            |result| result.dropped = 1,
            |result| result.unavailable = 1,
        ];
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut result = sampled_measurements();
            mutate(&mut result);
            assert!(!valid_measurements(&result, 1), "mutation {index}");
        }
    }

    #[test]
    fn synthetic_timing_validation_accepts_zero_and_rejects_missing_duplicate_extra_and_invalid() {
        let (timing, expected) = synthetic();
        assert_eq!(
            duration_values(&timing, &expected),
            Some(vec![0.0, 1.0, 2.0])
        );
        let mut missing = timing.clone();
        missing.passes.pop();
        assert_eq!(duration_values(&missing, &expected), None);
        let mut duplicate = timing.clone();
        duplicate.passes[1].pass_id = duplicate.passes[0].pass_id;
        assert_eq!(duration_values(&duplicate, &expected), None);
        let mut extra = timing.clone();
        extra.passes.push(PassTiming {
            pass_id: expected[0],
            duration_ns: Some(3.0),
        });
        assert_eq!(duration_values(&extra, &expected), None);
        for duration in [None, Some(f64::NAN), Some(-1.0)] {
            let mut invalid = timing.clone();
            invalid.passes[0].duration_ns = duration;
            assert_eq!(duration_values(&invalid, &expected), None);
        }
    }
}
