//! The whole-frame GPU timestamp envelope of the qualification matrix.
//!
//! Pass-level timings leave out clears and copies. The envelope instead writes a timestamp from
//! an empty compute pass before the whole frame's commands and another from an empty compute pass
//! after them, in the same command buffer, so the difference spans scene, post, copies and
//! clears. It needs only `TIMESTAMP_QUERY` (pass-level writes). Missing support is reported as
//! `Unavailable`, never as zero. Frames are timed one at a time (submit, wait, read), so the
//! numbers are per-frame latency on an idle queue, not overlapped throughput.
use crate::device::GpuContext;

/// Warm-up and measured frame counts of the cost runs.
pub(super) const WARM_FRAMES: usize = 30;
pub(super) const MEASURED_FRAMES: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Envelope {
    Unavailable,
    Measured { median_ms: f64, p95_ms: f64 },
}

impl std::fmt::Display for Envelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "unavailable"),
            Self::Measured { median_ms, p95_ms } => write!(f, "{median_ms:.3} / {p95_ms:.3}"),
        }
    }
}

/// Median and nearest-rank 95th percentile of per-frame GPU milliseconds.
pub(super) fn summarize(samples: &[f64]) -> Envelope {
    if samples.is_empty() {
        return Envelope::Unavailable;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let median_ms = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        f64::midpoint(sorted[n / 2 - 1], sorted[n / 2])
    };
    // Nearest rank: the smallest value with at least 95% of the samples at or below it.
    let rank = (n * 95).div_ceil(100).max(1);
    Envelope::Measured {
        median_ms,
        p95_ms: sorted[rank - 1],
    }
}

pub(super) struct FrameEnvelope {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    staging: wgpu::Buffer,
    period_ns: f32,
}

impl FrameEnvelope {
    /// `None` when the device lacks `TIMESTAMP_QUERY`.
    pub(super) fn new(context: &GpuContext) -> Option<Self> {
        let device = context.device();
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 16,
                usage,
                mapped_at_creation: false,
            })
        };
        Some(Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("frame envelope"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            }),
            resolve: buffer(
                "frame envelope resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            staging: buffer(
                "frame envelope staging",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            period_ns: context.queue().get_timestamp_period(),
        })
    }

    /// Writes the start timestamp; record before the frame's first command.
    pub(super) fn begin(&self, encoder: &mut wgpu::CommandEncoder) {
        drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("frame envelope begin"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: None,
            }),
        }));
    }

    /// Writes the end timestamp and stages the result; record after the frame's last command.
    pub(super) fn end(&self, encoder: &mut wgpu::CommandEncoder) {
        drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("frame envelope end"),
            timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: None,
                end_of_pass_write_index: Some(1),
            }),
        }));
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.staging, 0, 16);
    }

    /// Waits for the submitted frame and returns its envelope in milliseconds.
    pub(super) fn read(&self, device: &wgpu::Device) -> Option<f64> {
        let slice = self.staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.expect("map envelope"));
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let ticks: [u64; 2] = {
            let data = slice.get_mapped_range().ok()?;
            [
                u64::from_le_bytes(data[0..8].try_into().ok()?),
                u64::from_le_bytes(data[8..16].try_into().ok()?),
            ]
        };
        self.staging.unmap();
        #[expect(
            clippy::cast_precision_loss,
            reason = "tick differences are far below 2^52"
        )]
        let delta = ticks[1].checked_sub(ticks[0])? as f64;
        Some(delta * f64::from(self.period_ns) / 1.0e6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_is_the_median_and_the_nearest_rank_95th_percentile() {
        let samples: Vec<f64> = (1..=120).rev().map(f64::from).collect();
        assert_eq!(
            summarize(&samples),
            Envelope::Measured {
                median_ms: 60.5,
                p95_ms: 114.0
            }
        );
        assert_eq!(
            summarize(&[3.0, 1.0, 2.0]),
            Envelope::Measured {
                median_ms: 2.0,
                p95_ms: 3.0
            }
        );
    }

    #[test]
    fn no_samples_are_unavailable_not_zero() {
        assert_eq!(summarize(&[]), Envelope::Unavailable);
        assert_eq!(Envelope::Unavailable.to_string(), "unavailable");
    }
}
