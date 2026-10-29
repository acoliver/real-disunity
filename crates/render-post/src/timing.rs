//! Optional pass-boundary queries. Collection never polls or waits for the GPU.
use std::sync::mpsc;

use crate::PassSlot;

pub const TIMING_RING_SIZE: usize = 3;
const MAX_PASSES: u32 = 1024;
const BUFFER_BYTES: u64 = MAX_PASSES as u64 * 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimingMode {
    #[default]
    Disabled,
    Optional,
    Required,
}
impl TimingMode {
    /// Apply before requesting the device. Pass boundaries need no inside-pass feature.
    pub const fn features(self) -> wgpu::Features {
        match self {
            Self::Disabled => wgpu::Features::empty(),
            Self::Optional | Self::Required => wgpu::Features::TIMESTAMP_QUERY,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingSupport {
    Disabled,
    Unsupported,
    Supported,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassId {
    Geometry(PassSlot),
    Resolve,
    Stage(StageId),
    Operation(crate::contracts::RasterId),
}
/// Provider-declared identity, independent of schedule position or target size.
/// Repeated operations use their logical level, not their execution index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageId {
    pub name: &'static str,
    pub level: u32,
}
impl StageId {
    pub const fn new(name: &'static str, level: u32) -> Self {
        Self { name, level }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PassTiming {
    pub pass_id: PassId,
    /// None for all-zero or reversed counters. Equal nonzero counters yield zero
    /// nanoseconds because the interval can be shorter than timer resolution.
    pub duration_ns: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FrameTiming {
    pub frame_id: u64,
    pub passes: Vec<PassTiming>,
}
#[derive(Debug, thiserror::Error)]
pub enum TimingError {
    #[error("required timestamp-query capability was not enabled on the device")]
    Unsupported,
    #[error("timestamp readback failed: {0}")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("timestamp callback disconnected")]
    Callback,
}

struct Slot {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    transaction: Option<worldrunner_render_gpu::submission::Publications>,
    pending: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
    frame_id: u64,
    sequence: u64,
    passes: Vec<PassId>,
}
impl Slot {
    fn new(device: &wgpu::Device) -> Self {
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: BUFFER_BYTES,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("frame timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: MAX_PASSES * 2,
            }),
            resolve: buffer(
                "timestamp resolve",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            readback: buffer(
                "timestamp readback",
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            ),
            transaction: None,
            pending: None,
            frame_id: 0,
            sequence: 0,
            passes: Vec::new(),
        }
    }
}

pub(crate) struct Stamp {
    queries: wgpu::QuerySet,
    begin: u32,
}
impl Stamp {
    pub fn render(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(self.begin),
            end_of_pass_write_index: Some(self.begin + 1),
        }
    }
    pub fn compute(&self) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(self.begin),
            end_of_pass_write_index: Some(self.begin + 1),
        }
    }
}

/// Three unread frames are retained. When full, new timing samples are declined,
/// not rendering. Frames exceeding 1024 passes are dropped whole with the same counter.
/// All-zero and reversed query pairs retain their IDs but have no duration and
/// increment `unavailable_passes`. Counter width is not exposed by wgpu,
/// so wrap cannot be resolved portably. Valid pairs require end >= begin, at least
/// one nonzero counter and an interval shorter than one hardware counter cycle.
/// Equal nonzero counters report zero duration, allowing timer-resolution limits.
/// Begin/finish must bracket one submitted encoder; successfully encoded frames
/// must be submitted in preparation order, just like resident feature uploads.
pub struct PassTimer {
    support: TimingSupport,
    slots: Vec<Slot>,
    active: Option<usize>,
    overflow: bool,
    sequence: u64,
    dropped: u64,
    unavailable: u64,
    period_ns: f64,
}
impl PassTimer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mode: TimingMode,
    ) -> Result<Self, TimingError> {
        let available = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let support = match (mode, available) {
            (TimingMode::Disabled, _) => TimingSupport::Disabled,
            (TimingMode::Required, false) => return Err(TimingError::Unsupported),
            (TimingMode::Optional, false) => TimingSupport::Unsupported,
            (_, true) => TimingSupport::Supported,
        };
        Ok(Self {
            support,
            slots: if support == TimingSupport::Supported {
                (0..TIMING_RING_SIZE).map(|_| Slot::new(device)).collect()
            } else {
                Vec::new()
            },
            active: None,
            overflow: false,
            sequence: 0,
            dropped: 0,
            unavailable: 0,
            period_ns: f64::from(queue.get_timestamp_period()),
        })
    }
    pub const fn support(&self) -> TimingSupport {
        self.support
    }
    pub const fn dropped_frames(&self) -> u64 {
        self.dropped
    }
    pub const fn unavailable_passes(&self) -> u64 {
        self.unavailable
    }
    fn discard_cancelled(&mut self) {
        use worldrunner_render_gpu::submission::TransactionStatus;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot
                .transaction
                .as_ref()
                .is_some_and(|owner| owner.status() == TransactionStatus::Cancelled)
            {
                slot.pending = None;
                slot.passes.clear();
                slot.transaction = None;
                if self.active == Some(index) {
                    self.active = None;
                }
            }
        }
    }
    pub fn begin_frame(&mut self, frame_id: u64) {
        self.discard_cancelled();
        assert!(
            self.active.is_none(),
            "finish timing before beginning another frame"
        );
        if self.support != TimingSupport::Supported {
            return;
        }
        self.active = self.slots.iter().position(|slot| slot.pending.is_none());
        self.overflow = false;
        if let Some(index) = self.active {
            let slot = &mut self.slots[index];
            slot.transaction = worldrunner_render_gpu::submission::Publications::current();
            slot.frame_id = frame_id;
            slot.sequence = self.sequence;
            slot.passes.clear();
            self.sequence += 1;
        } else {
            self.dropped += 1;
        }
    }
    pub(crate) fn stamp(&mut self, pass_id: PassId) -> Option<Stamp> {
        let slot = &mut self.slots[self.active?];
        let begin = u32::try_from(slot.passes.len()).expect("bounded timestamp count") * 2;
        if begin == MAX_PASSES * 2 {
            self.overflow = true;
            return None;
        }
        slot.passes.push(pass_id);
        Some(Stamp {
            queries: slot.queries.clone(),
            begin,
        })
    }
    pub fn finish_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(index) = self.active.take() else {
            return;
        };
        let slot = &mut self.slots[index];
        if self.overflow {
            self.dropped += 1;
            return;
        }
        if slot.passes.is_empty() {
            return;
        }
        let count = u32::try_from(slot.passes.len()).expect("bounded timestamp count") * 2;
        let bytes = u64::from(count) * 8;
        encoder.resolve_query_set(&slot.queries, 0..count, &slot.resolve, 0);
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, bytes);
        let (send, receive) = mpsc::sync_channel(1);
        encoder.map_buffer_on_submit(
            &slot.readback,
            wgpu::MapMode::Read,
            0..bytes,
            move |result| {
                let _ = send.send(result);
            },
        );
        slot.pending = Some(receive);
    }
    /// Nonblocking. The host drives progress with its normal device polling.
    /// Results preserve encoding order, including when callbacks finish out of order.
    pub fn take_ready(&mut self) -> Result<Vec<FrameTiming>, TimingError> {
        self.discard_cancelled();
        let mut ordered: Vec<_> = self
            .slots
            .iter_mut()
            .filter(|slot| {
                slot.pending.is_some()
                    && slot.transaction.as_ref().is_none_or(|owner| {
                        owner.status()
                            == worldrunner_render_gpu::submission::TransactionStatus::Submitted
                    })
            })
            .collect();
        ordered.sort_by_key(|slot| slot.sequence);
        let mut results = Vec::new();
        for slot in ordered {
            match slot.pending.as_ref().unwrap().try_recv() {
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Err(TimingError::Callback),
                Ok(result) => {
                    slot.pending = None;
                    result?;
                }
            }
            let bytes = slot
                .readback
                .slice(..slot.passes.len() as u64 * 16)
                .get_mapped_range()
                .expect("completed timestamp mapping");
            let passes = bytes
                .chunks_exact(16)
                .zip(&slot.passes)
                .map(|(pair, &pass_id)| {
                    let begin = u64::from_le_bytes(pair[..8].try_into().unwrap());
                    let end = u64::from_le_bytes(pair[8..].try_into().unwrap());
                    #[cfg(test)]
                    if end < begin {
                        eprintln!(
                            "reversed timestamp frame={} pass={pass_id:?} begin={begin} end={end}",
                            slot.frame_id
                        );
                    }
                    PassTiming {
                        pass_id,
                        duration_ns: ticks_to_ns(begin, end, self.period_ns),
                    }
                })
                .collect::<Vec<_>>();
            drop(bytes);
            slot.readback.unmap();
            self.unavailable += passes
                .iter()
                .filter(|pass| pass.duration_ns.is_none())
                .count() as u64;
            results.push(FrameTiming {
                frame_id: slot.frame_id,
                passes,
            });
        }
        Ok(results)
    }
}
fn ticks_to_ns(begin: u64, end: u64, period: f64) -> Option<f64> {
    if begin == 0 && end == 0 {
        return None;
    }
    // Unknown counter modulus: reject reversed pairs rather than inventing a duration.
    // Precision is applied to the delta, not absolute ticks.
    let words = end.checked_sub(begin)?.to_le_bytes();
    let delta = f64::from(u32::from_le_bytes(words[..4].try_into().unwrap()))
        + f64::from(u32::from_le_bytes(words[4..].try_into().unwrap())) * 4_294_967_296.0;
    Some(delta * period)
}

#[cfg(test)]
#[path = "timing_tests.rs"]
mod tests;
