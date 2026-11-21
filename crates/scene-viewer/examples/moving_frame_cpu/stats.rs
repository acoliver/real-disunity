//! Order statistics of the measured frames.
use std::time::Duration;

/// One frame's cost: the time inside host calls, and the thread's CPU time over the whole wait.
pub struct Sample {
    pub active: Duration,
    pub cpu: Option<Duration>,
}

/// The value at `numerator / 100` of the way through the sorted values (nearest rank).
pub fn percentile(sorted: &[Duration], numerator: usize) -> Duration {
    assert!(!sorted.is_empty(), "a percentile needs at least one sample");
    sorted[(sorted.len() * numerator).div_ceil(100).max(1) - 1]
}

fn sorted(mut values: Vec<Duration>) -> Vec<Duration> {
    values.sort();
    values
}

fn line(label: &str, values: Vec<Duration>) {
    let values = sorted(values);
    let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
    println!(
        "{label:<22} median {:>7.3} ms   p90 {:>7.3} ms   min {:>7.3} ms   max {:>7.3} ms",
        milliseconds(percentile(&values, 50)),
        milliseconds(percentile(&values, 90)),
        milliseconds(values[0]),
        milliseconds(values[values.len() - 1]),
    );
}

pub fn report(summary: &str, samples: &[Sample]) {
    println!("moving-frame CPU: {summary}");
    println!("frames measured: {}", samples.len());
    line(
        "host calls (wall)",
        samples.iter().map(|sample| sample.active).collect(),
    );
    let cpu: Vec<Duration> = samples.iter().filter_map(|sample| sample.cpu).collect();
    if cpu.len() == samples.len() {
        line("thread CPU time", cpu);
    }
}

/// When measurement began and ended, in seconds since the process started. A profiler's samples
/// of the same run can be cut to this window to leave out start-up and warm-up.
pub fn report_timeline(warmed: Duration, finished: Duration) {
    println!(
        "timeline: measurement ran from {:.3} s to {:.3} s after start",
        warmed.as_secs_f64(),
        finished.as_secs_f64()
    );
}

/// What the last measured frame recorded, so the scene's shape is visible next to the timings.
pub fn report_draws(draws: &worldrunner_render_scene::draw::PassDrawStatistics) {
    let describe = |label: &str, count: worldrunner_scene_viewer::DrawCount| {
        println!(
            "last frame {label:<16} {:>6} calls {:>7} instances",
            count.calls, count.instances
        );
    };
    describe("terrain", draws.terrain);
    describe("opaque objects", draws.opaque_objects.meshes);
    describe("prepass objects", draws.prepass_objects.meshes);
    for (cascade, count) in draws.shadow_cascades.iter().enumerate() {
        describe(&format!("shadow cascade {cascade}"), *count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let values: Vec<Duration> = (1..=10).map(Duration::from_millis).collect();
        assert_eq!(percentile(&values, 50), Duration::from_millis(5));
        assert_eq!(percentile(&values, 90), Duration::from_millis(9));
        assert_eq!(percentile(&values, 100), Duration::from_millis(10));
        assert_eq!(percentile(&values[..1], 90), Duration::from_millis(1));
    }
}
