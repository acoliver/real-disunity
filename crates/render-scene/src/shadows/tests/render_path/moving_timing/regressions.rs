use super::*;
use worldrunner_render_post::{PassEntry, PassSlot, StageId};

pub(super) fn expected(path: Path) -> Vec<PassId> {
    let mut slots = vec![PassSlot::DepthNormalPrepass];
    slots.extend((0..10).map(PassSlot::DepthPyramid));
    slots.extend([PassSlot::Ssao, PassSlot::SsaoBlur]);
    if path == Path::Render {
        slots.push(PassSlot::OpaqueLighting);
    }
    slots.push(PassSlot::Opaque);
    let mut ids: Vec<_> = slots
        .into_iter()
        .map(|slot| PassId::Geometry(PassEntry::new(slot)))
        .collect();
    ids.push(PassId::Stage(StageId::new("tone", 0)));
    ids
}

fn complete(path: Path) -> Measurements {
    let expected_passes = expected(path);
    let observations: Vec<_> = (WARMUP..WARMUP + SAMPLES)
        .map(|view_index| Observation {
            view_index,
            frame_id: u64::try_from(view_index * 2 + path.index() + 1).unwrap(),
            passes: expected_passes
                .iter()
                .filter_map(|id| match id {
                    PassId::Geometry(entry) => Some(entry.slot()),
                    _ => None,
                })
                .collect(),
            draws: crate::draw::PassDrawStatistics::default(),
        })
        .collect();
    let timings = observations
        .iter()
        .map(|sample| FrameTiming {
            frame_id: sample.frame_id,
            passes: expected_passes
                .iter()
                .map(|id| PassTiming {
                    pass_id: *id,
                    duration_ns: Some(0.0),
                })
                .collect(),
        })
        .collect();
    Measurements {
        path,
        observations,
        expected_passes,
        timings,
        timing_support: Some(TimingSupport::Supported),
        ..Measurements::default()
    }
}

#[test]
fn empty_timestamp_frame_is_unavailable() {
    assert_eq!(
        durations(&FrameTiming {
            frame_id: 61,
            passes: vec![]
        }),
        None
    );
}

#[test]
fn summary_requires_every_expected_pass_even_if_always_omitted() {
    for path in [Path::Forward, Path::Render] {
        let mut result = complete(path);
        assert!(complete_summary(&result, SAMPLES));
        assert!(!result.expected_passes.is_empty());
        for timing in &mut result.timings {
            timing.passes.remove(0);
        }
        assert!(
            !complete_summary(&result, SAMPLES),
            "{path:?}: omitted prepass"
        );
    }
}

#[test]
fn summary_refuses_empty_extra_duplicate_and_changed_full_identities() {
    for path in [Path::Forward, Path::Render] {
        let mut empty = complete(path);
        for timing in &mut empty.timings {
            timing.passes.clear();
        }
        assert!(!complete_summary(&empty, SAMPLES));
        for id in [
            PassId::Geometry(PassEntry::nth(PassSlot::DepthNormalPrepass, 1)),
            PassId::Geometry(PassEntry::new(PassSlot::DepthPyramid(99))),
            PassId::Stage(StageId::new("tone", 1)),
            PassId::Resolve,
        ] {
            let mut result = complete(path);
            result.timings[0].passes[0].pass_id = id;
            assert!(!complete_summary(&result, SAMPLES), "{id:?}");
        }
        let mut extra = complete(path);
        let entry = extra.timings[0].passes[0].clone();
        extra.timings[0].passes.push(entry);
        assert!(!complete_summary(&extra, SAMPLES));
        let mut duplicate = complete(path);
        duplicate.timings[0].passes[1] = duplicate.timings[0].passes[0].clone();
        assert!(!complete_summary(&duplicate, SAMPLES));
    }
}

#[test]
fn zero_duration_pairs_are_complete_but_none_drops_and_missing_frames_are_not() {
    for path in [Path::Forward, Path::Render] {
        let result = complete(path);
        assert!(
            durations(&result.timings[0])
                .unwrap()
                .iter()
                .all(|(_, ns)| *ns == 0.0)
        );
        assert!(complete_summary(&result, SAMPLES));
        let mut unavailable = complete(path);
        unavailable.timings[5].passes[3].duration_ns = None;
        assert!(!complete_summary(&unavailable, SAMPLES));
        let mut dropped = complete(path);
        dropped.dropped_timing_frames = 1;
        assert!(!complete_summary(&dropped, SAMPLES));
        let mut counters = complete(path);
        counters.unavailable_timing_passes = 1;
        assert!(!complete_summary(&counters, SAMPLES));
        let mut missing = complete(path);
        missing.timings.pop();
        assert!(!complete_summary(&missing, SAMPLES));
        let mut unsupported = complete(path);
        unsupported.timing_support = Some(TimingSupport::Unsupported);
        assert!(!complete_summary(&unsupported, SAMPLES));
    }
}

#[test]
fn summary_checks_view_frame_and_sample_association_at_warmup_boundary() {
    for path in [Path::Forward, Path::Render] {
        let result = complete(path);
        assert_eq!(result.path, path);
        assert_eq!(result.observations.len(), 120);
        assert_eq!(result.observations[0].view_index, 30);
        assert_eq!(result.observations[119].view_index, 149);
        assert_eq!(result.timings[0].frame_id, 61 + path.index() as u64);
        assert_eq!(result.timings[119].frame_id, 299 + path.index() as u64);
        assert!(complete_summary(&result, SAMPLES));
        let mut warmup = complete(path);
        warmup.timings[0].frame_id -= 2;
        assert!(!complete_summary(&warmup, SAMPLES));
        let mut repeated = complete(path);
        repeated.timings[1].frame_id = repeated.timings[0].frame_id;
        assert!(!complete_summary(&repeated, SAMPLES));
        let mut wrong_sample = complete(path);
        wrong_sample.observations[0].view_index = 29;
        assert!(!complete_summary(&wrong_sample, SAMPLES));
        let mut wrong_path = complete(path);
        wrong_path.timings[0].frame_id += 1;
        assert!(!complete_summary(&wrong_path, SAMPLES));
    }
}

#[test]
fn gpu_collection_retains_each_sample_and_omits_render_opaque_object_draws() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let results = collect(&context, &super::super::source(false, true), WARMUP + 3);
    for (path, result) in [Path::Forward, Path::Render].into_iter().zip(&results) {
        assert_eq!(result.observations.len(), 3, "{path:?}: sampled evidence");
        assert_eq!(result.expected_passes, expected(path));
        assert_eq!(result.host_encode_submit.len(), 3);
        assert_eq!(result.completed_frame.len(), 3);
        assert_eq!(result.timing_support, Some(TimingSupport::Unsupported));
        assert!(result.timings.is_empty());
        assert!(!complete_summary(result, SAMPLES));
        for (index, sample) in result.observations.iter().enumerate() {
            assert_eq!(sample.view_index, WARMUP + index);
            assert_eq!(
                sample.frame_id,
                ((WARMUP + index) * 2 + path.index() + 1) as u64
            );
            let count = |slot| sample.passes.iter().filter(|pass| **pass == slot).count();
            assert_eq!(count(PassSlot::DepthNormalPrepass), 1);
            assert_eq!(count(PassSlot::OpaqueGbuffer), 0);
            assert_eq!(
                count(PassSlot::OpaqueLighting),
                usize::from(path == Path::Render)
            );
            assert!(sample.draws.terrain.calls > 0);
            let objects = match path {
                Path::Forward => sample.draws.opaque_objects,
                Path::Render => sample.draws.prepass_objects,
            };
            assert!(objects.meshes.calls > 0 && objects.meshes.instances > 0);
            assert!(objects.impostors.calls > 0 && objects.impostors.instances > 0);
            if path == Path::Render {
                assert_eq!(sample.draws.opaque_objects.meshes.calls, 0);
                assert_eq!(sample.draws.opaque_objects.meshes.instances, 0);
                assert_eq!(sample.draws.opaque_objects.impostors.calls, 0);
                assert_eq!(sample.draws.opaque_objects.impostors.instances, 0);
            }
        }
    }
}

#[test]
fn gpu_optional_timestamps_match_the_independent_path_schedule() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = timing_context();
    let support = if context
        .device()
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        TimingSupport::Supported
    } else {
        TimingSupport::Unsupported
    };
    let results = collect(&context, &super::super::source(false, true), WARMUP + 3);
    for result in &results {
        assert_eq!(result.timing_support, Some(support));
        assert_eq!(result.expected_passes, expected(result.path));
        assert_eq!(result.observations.len(), 3);
        assert!(!complete_summary(result, SAMPLES));
        if support != TimingSupport::Supported {
            assert!(result.timings.is_empty());
            continue;
        }
        assert_eq!(result.timings.len(), 3);
        for (index, timing) in result.timings.iter().enumerate() {
            assert_eq!(timing.frame_id, frame_id(WARMUP + index, result.path));
            assert_eq!(timing.passes.len(), result.expected_passes.len());
            for expected in &result.expected_passes {
                assert_eq!(
                    timing
                        .passes
                        .iter()
                        .filter(|pass| pass.pass_id == *expected)
                        .count(),
                    1
                );
            }
        }
    }
}

#[test]
fn expected_schedule_and_every_sample_are_required_for_complete_coverage() {
    for path in [Path::Forward, Path::Render] {
        let mut absent_schedule = complete(path);
        absent_schedule.expected_passes.clear();
        assert!(!complete_summary(&absent_schedule, SAMPLES));
        let mut absent_sample = complete(path);
        absent_sample.observations.remove(5);
        assert!(!complete_summary(&absent_sample, SAMPLES));
        let mut wrong_sample_id = complete(path);
        wrong_sample_id.observations[0].frame_id += 2;
        assert!(!complete_summary(&wrong_sample_id, SAMPLES));
        let mut reordered = complete(path);
        reordered.timings[0].passes.reverse();
        assert!(complete_summary(&reordered, SAMPLES));
        assert_eq!(frame_id(29, path), 59 + path.index() as u64);
        assert_eq!(frame_id(30, path), 61 + path.index() as u64);
        assert_eq!(frame_id(149, path), 299 + path.index() as u64);
    }
}
