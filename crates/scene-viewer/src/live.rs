//! Caller-owned simulation and pathless frame contributions for the desktop host.

mod ingress;
mod session;
mod startup;
#[cfg(test)]
mod tests;
pub(crate) use session::Session;

use std::{sync::mpsc::Receiver, time::Duration};

use worldrunner_render_plan::SceneTexture;
use worldrunner_render_scene::{SceneView, Viewport, dynamic::DynamicQuad, offscreen::RgbaImage};

/// Native-host-independent logical input. Character keys are lowercase; named
/// keys use lowercase names such as `arrowleft`, `space`, and `escape`.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveInputEvent {
    Key {
        key: String,
        pressed: bool,
        repeat: bool,
    },
    Text(String),
    Focused(bool),
    Resized([u32; 2]),
    CloseRequested,
    /// Delivered to the driver, which selects whether to capture its contribution.
    CaptureRequested(u64),
}

/// Timestamp provenance. Replay uses a logical clock independent of host acquisition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngressOrigin {
    /// Host event-loop acquisition time, without a hardware timestamp claim.
    OsEventLoop,
    /// Scheduled frame times `fixed_dt.unwrap_or(frame_interval)`.
    ScheduledReplay,
    /// Host receiver drain time. The producer's original timestamp is unknown.
    ChannelDelivery,
}

/// An immutable transport observation delivered by reference before update.
///
/// Sequence starts at 1 and increases in delivery order, including timestamp ties.
/// Host timestamps are unscaled time since successful initialize; earlier events
/// clamp to zero. Replay timestamps use their separate deterministic logical clock.
/// Each clock is nondecreasing. Strict consumers must reject mixed clocks or map
/// them explicitly before native submission. No held state or gameplay edge is derived.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveInputSample {
    pub sequence: u64,
    pub timestamp: Duration,
    pub origin: IngressOrigin,
    pub event: LiveInputEvent,
}

/// Simulation delta origin selected independently from input provenance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LiveClock {
    /// Preserve the historical zero first delta and optional fixed replay delta.
    #[default]
    Legacy,
    /// Start real-time deltas at the exact OS acquisition origin after initialize.
    /// Fixed replay deltas are incompatible with this clock.
    Acquisition,
    /// Opt-in Acquisition variant. After initialize, the driver's actual initialized
    /// scene is prepared, rendered, presented and completed on the GPU before one
    /// origin is set. Queued events are pumped first; any key or text event acquired
    /// before that origin rejects startup. The first gameplay delta includes all time
    /// after the origin. Fixed replay deltas are incompatible with this clock.
    ReadyPrewarmed,
}

impl LiveClock {
    /// Whether deltas are measured from a real acquisition origin.
    #[must_use]
    pub const fn real_time(self) -> bool {
        matches!(self, Self::Acquisition | Self::ReadyPrewarmed)
    }
}

/// One deterministic synthetic event, delivered at the zero-based update index.
#[derive(Clone, Debug)]
pub struct ScheduledInput {
    pub frame: u64,
    pub event: LiveInputEvent,
}

/// Explicit desktop timing, termination and synthetic ingress settings.
pub struct LiveOptions {
    pub title: String,
    pub size: [u32; 2],
    pub frame_interval: Duration,
    pub fixed_dt: Option<Duration>,
    pub max_frames: Option<u64>,
    pub scheduled_input: Vec<ScheduledInput>,
    pub input: Option<Receiver<LiveInputEvent>>,
}

impl LiveOptions {
    pub fn new(title: impl Into<String>, size: [u32; 2]) -> Self {
        Self {
            title: title.into(),
            size,
            frame_interval: Duration::from_millis(16),
            fixed_dt: None,
            max_frames: None,
            scheduled_input: Vec::new(),
            input: None,
        }
    }
}

/// One simulation update. The viewport describes the bounded render attachment.
#[derive(Clone, Debug)]
pub struct LiveTick {
    pub frame: u64,
    pub elapsed: Duration,
    pub dt: Duration,
    pub viewport: Viewport,
    pub input: Vec<LiveInputEvent>,
}

/// A complete contribution. Texture tables are retained until replaced by Some.
/// Clear channels are linear RGBA in 0..=1. A capture reads this presented frame.
pub struct LiveFrame {
    pub view: SceneView,
    pub quads: Vec<DynamicQuad>,
    pub textures: Option<Vec<SceneTexture>>,
    pub clear: [f64; 4],
    pub capture: Option<u64>,
    pub exit: bool,
}

/// Actual tightly packed sRGB GPU pixels correlated to a caller-owned request ID.
pub struct LiveCapture {
    pub id: u64,
    pub image: RgbaImage,
}

/// Successful presentation of one update; skipped acquisitions produce no receipt.
pub struct LiveReceipt {
    pub frame: u64,
    pub elapsed: Duration,
    pub capture: Option<LiveCapture>,
}

/// Application-owned simulation, extraction, and evidence publication.
/// The same callbacks may be called directly by a headless replay host.
pub trait LiveDriver {
    /// Pure timing preference queried before initialization. The default preserves legacy timing.
    fn clock(&self) -> LiveClock {
        LiveClock::Legacy
    }

    /// Called once before the first update, with the actual render viewport.
    fn initialize(&mut self, _viewport: Viewport) -> Result<(), String> {
        Ok(())
    }

    /// Observes each ordered transition before update, including host controls.
    /// The default preserves legacy consumers. An error aborts the live host before
    /// update/presentation; earlier observations are not rolled back. Consumers
    /// requiring atomic native batches must stage observations themselves.
    fn ingress(&mut self, _sample: &LiveInputSample) -> Result<(), String> {
        Ok(())
    }

    /// `ReadyPrewarmed` only: the actual initialized scene, extracted without advancing
    /// simulation, submitting input, capturing or exiting. `None` rejects startup.
    fn prepare_initial(&mut self, _viewport: Viewport) -> Result<Option<LiveFrame>, String> {
        Ok(None)
    }

    /// `ReadyPrewarmed` only: called once, immediately after the clock origin is set and
    /// before the first update. It carries no gameplay frame, update or capture identity.
    fn ready(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String>;

    /// Called after presentation. Image encoding and filesystem policy remain caller-owned.
    fn presented(&mut self, _receipt: LiveReceipt) -> Result<(), String> {
        Ok(())
    }
}
