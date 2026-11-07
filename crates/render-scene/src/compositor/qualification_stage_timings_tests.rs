//! The per-stage GPU timing breakdown of the qualification matrix (issue #256, section 7).
//!
//! `qualification_stage_timings` is an explicit measurement, not a CI check. For modes M3, M4
//! and M5 at each output and scale it renders the composite scene through
//! `ConfiguredFrameRenderer` with the pass-level timing API (`configure_timing`, `begin_timing`,
//! `take_timings`), and prints the median GPU milliseconds of every timed pass over the
//! measured frames, the sum of those medians and the whole-frame envelope median. The gap
//! between the two is the work that has no timing slot (clears, copies, scene passes of the
//! test scene). Pass names are printed as the timing API reports them: a stage by its declared
//! name (with its level when that is not zero), any other pass by the `Debug` form of its id.
//! The shared `PassId` display spelling also selects the encoder label.
//!
//! The scene rasterisation of the test scene sits inside the envelope but outside every timed
//! pass. Each frame is submitted and waited for alone, so the numbers are per-frame latency on
//! an idle queue. Run it as
//! `cargo test -p worldrunner-render-scene qualification_stage_timings -- --ignored --nocapture
//! --test-threads=1`. The `RD_QUAL_*` variables of the matrix apply: `OUTPUTS` (default
//! `1920x1080`), `SCALES` (default `500000,770000`), `MODES` (default `M3,M4,M5`),
//! `TIMING_WARM` and `TIMING_FRAMES`. Lavapipe numbers only check that the test runs.
use super::{
    CompositeScene, Envelope, Harness, MEASURED_FRAMES, Mode, WARM_FRAMES, env_list, env_one,
    env_outputs, open_context, summarize,
};
use worldrunner_render_post::{FrameTiming, PassId, TimingMode, TimingSupport};

/// A pass name and the GPU milliseconds one frame spent in it.
type StageSample = (String, f64);

#[derive(Clone, Debug, PartialEq)]
struct StageMedian {
    name: String,
    median_ms: f64,
    /// Number of measured frames that reported this pass.
    frames: usize,
}

fn pass_name(id: &PassId) -> String {
    id.to_string()
}

/// One frame's samples in encoding order. Passes without a duration are left out, and passes that
/// share a name within the frame are added together so that each name has one sample per frame.
fn frame_samples(timing: &FrameTiming) -> Vec<StageSample> {
    let mut samples: Vec<StageSample> = Vec::new();
    for pass in &timing.passes {
        let Some(nanoseconds) = pass.duration_ns else {
            continue;
        };
        let (name, milliseconds) = (pass_name(&pass.pass_id), nanoseconds / 1.0e6);
        match samples.iter_mut().find(|(known, _)| *known == name) {
            Some(entry) => entry.1 += milliseconds,
            None => samples.push((name, milliseconds)),
        }
    }
    samples
}

/// The median of each name across the frames, in order of first appearance.
fn median_by_name(frames: &[Vec<StageSample>]) -> Vec<StageMedian> {
    let mut names: Vec<&str> = Vec::new();
    for (name, _) in frames.iter().flatten() {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    names
        .into_iter()
        .filter_map(|name| {
            let values: Vec<f64> = frames
                .iter()
                .flatten()
                .filter(|(known, _)| known == name)
                .map(|(_, milliseconds)| *milliseconds)
                .collect();
            match summarize(&values) {
                Envelope::Measured { median_ms, .. } => Some(StageMedian {
                    name: name.to_owned(),
                    median_ms,
                    frames: values.len(),
                }),
                Envelope::Unavailable => None,
            }
        })
        .collect()
}

/// Per-frame stage samples and whole-frame envelopes of the measured frames.
struct Measured {
    stages: Vec<Vec<StageSample>>,
    envelopes: Vec<f64>,
}

/// Static camera, one frame at a time; the warm-up frames drain their timings and are discarded.
fn measure(harness: &mut Harness<'_>, (warm, measured): (usize, usize)) -> Measured {
    let device = harness.context.device();
    let mut result = Measured {
        stages: Vec::new(),
        envelopes: Vec::new(),
    };
    for index in 0..warm + measured {
        let envelope = harness.frame(0);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("wait for the frame");
        let timings = harness.renderer.take_timings().unwrap();
        if index >= warm {
            assert_eq!(timings.len(), 1, "frame {index}: one timing record");
            result.stages.extend(timings.iter().map(frame_samples));
            result.envelopes.extend(envelope);
        }
    }
    result
}

fn print_cell(cell: (Mode, [u32; 2], u32), harness: &Harness<'_>, counts: (usize, usize)) {
    let (mode, output, scale) = cell;
    println!(
        "stage timings {mode:?} {}x{} scale={scale} render={}x{} warm={} measured={}",
        output[0], output[1], harness.render[0], harness.render[1], counts.0, counts.1
    );
}

fn print_report(measured: &Measured) {
    let medians = median_by_name(&measured.stages);
    for stage in &medians {
        assert!(
            stage.median_ms.is_finite() && stage.median_ms >= 0.0,
            "{stage:?}"
        );
        println!(
            "  {:<44} {:>9.3} ms  (n={})",
            stage.name, stage.median_ms, stage.frames
        );
    }
    let sum: f64 = medians.iter().map(|stage| stage.median_ms).sum();
    println!("  {:<44} {sum:>9.3} ms", "sum of stage medians");
    match summarize(&measured.envelopes) {
        Envelope::Measured { median_ms, p95_ms } => {
            assert!(median_ms.is_finite() && p95_ms.is_finite());
            println!(
                "  {:<44} {median_ms:>9.3} ms  (p95 {p95_ms:.3})",
                "whole-frame median"
            );
            println!(
                "  {:<44} {:>9.3} ms",
                "gap (whole frame minus sum)",
                median_ms - sum
            );
        }
        Envelope::Unavailable => println!("  whole-frame median: unavailable"),
    }
}

#[test]
#[ignore = "per-stage GPU timing measurement: run explicitly with --ignored --nocapture"]
fn qualification_stage_timings() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let outputs = env_outputs("1920x1080");
    let scales: Vec<u32> = env_list("SCALES", "500000,770000");
    let modes: Vec<Mode> = env_list("MODES", "M3,M4,M5");
    let counts = (
        env_one("TIMING_WARM", WARM_FRAMES),
        env_one("TIMING_FRAMES", MEASURED_FRAMES),
    );
    let context = open_context();
    let scene = CompositeScene::new(context.device());
    for output in outputs {
        for (index, &scale) in scales.iter().enumerate() {
            for &mode in &modes {
                // Full-resolution TAA does not depend on the scale: measure it once per output.
                if mode == Mode::M5 && index > 0 {
                    continue;
                }
                let mut harness = Harness::new(&context, &scene, (mode, output, scale));
                let support = harness
                    .renderer
                    .renderer_mut()
                    .configure_timing(context.device(), context.queue(), TimingMode::Optional)
                    .unwrap();
                print_cell((mode, output, scale), &harness, counts);
                if support != TimingSupport::Supported {
                    println!("  pass timestamps unavailable on this device");
                    continue;
                }
                harness.timed = true;
                print_report(&measure(&mut harness, counts));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_render_post::{PassTiming, StageId};

    fn stage(name: &'static str, level: u32, nanoseconds: Option<f64>) -> PassTiming {
        PassTiming {
            pass_id: PassId::Stage(StageId::new(name, level)),
            duration_ns: nanoseconds,
        }
    }

    fn one(name: &str, milliseconds: f64) -> StageSample {
        (name.to_owned(), milliseconds)
    }

    #[test]
    fn the_median_is_taken_per_name_in_order_of_first_appearance() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let frames = vec![
            vec![one("b", 5.0), one("a", 1.0)],
            vec![one("b", 9.0), one("a", 3.0)],
            vec![one("b", 7.0), one("a", 2.0), one("c", 4.0)],
        ];
        let medians = median_by_name(&frames);
        let summary: Vec<(&str, f64, usize)> = medians
            .iter()
            .map(|m| (m.name.as_str(), m.median_ms, m.frames))
            .collect();
        assert_eq!(summary, [("b", 7.0, 3), ("a", 2.0, 3), ("c", 4.0, 1)]);
        assert!(median_by_name(&[]).is_empty());
    }

    #[test]
    fn authored_report_names_match_encoder_labels_without_merging_levels_or_occurrences() {
        use worldrunner_render_post::contracts::RasterId;
        let ids = [
            PassId::Stage(StageId::new("gaussian_downsample", 1)),
            PassId::Stage(StageId::new("gaussian_downsample", 2)),
            PassId::Stage(StageId::new("tone", 0)),
            PassId::Operation(RasterId::new(StageId::new("blur", 2), 0)),
            PassId::Operation(RasterId::new(StageId::new("blur", 2), 1)),
        ];
        let timing = FrameTiming {
            frame_id: 1,
            passes: ids
                .map(|pass_id| PassTiming {
                    pass_id,
                    duration_ns: Some(1.0e6),
                })
                .into(),
        };
        let samples = frame_samples(&timing);
        assert_eq!(samples.len(), ids.len());
        for ((name, milliseconds), id) in samples.iter().zip(ids) {
            assert_eq!(name, id.encoder_label().as_ref());
            assert_eq!(milliseconds.to_bits(), 1.0_f64.to_bits());
        }
        assert_eq!(samples[0].0, "gaussian_downsample level 1");
        assert_eq!(samples[1].0, "gaussian_downsample level 2");
        assert_eq!(samples[2].0, "tone");
    }

    #[test]
    fn a_frame_sums_passes_with_one_name_and_skips_passes_without_a_duration() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let timing = FrameTiming {
            frame_id: 1,
            passes: vec![
                stage("blur", 0, Some(1.0e6)),
                stage("blur", 0, Some(2.0e6)),
                stage("blur", 1, Some(0.5e6)),
                stage("lost", 0, None),
            ],
        };
        assert_eq!(
            frame_samples(&timing),
            [one("blur", 3.0), one("blur level 1", 0.5)]
        );
    }
}
