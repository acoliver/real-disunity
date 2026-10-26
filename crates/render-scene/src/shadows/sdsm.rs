//! Depth-driven fitting over the shared linear-depth and coverage products.
//! Prefix producers own geometry encoding; this module owns reduction, evidence
//! identity and adoption. Capture uses the submission authority's continuation.
//!
//! Stock depth fitting supports perspective views with finite positive near and
//! far clip distances. Orthographic views are not supported: cascade fitting uses
//! perspective rays, and a zero near clip is invalid. Use ordinary shadows for
//! orthographic rendering.
//!
//! Interactive latency is a minimum eligibility delay measured in source frames,
//! not a promise that GPU evidence will be ready on that frame. Completed older
//! evidence can be adopted later. Source frame identities must increase; `render`
//! supplies them automatically, while `render_depth_fitted` accepts host identities.
//! Re-enabling fitting replaces pending evidence and resets automatic identities.
//! Metadata replacement also invalidates pending evidence. Invalid settings,
//! readback failures and changed cascade layer counts return typed errors rather
//! than silently falling back. Rebuild the renderer, or disable fitting before
//! rendering a changed layer count.
//!
//! ```no_run
//! use worldrunner_render_scene::{device::GpuContext, shadows::{ShadowedScene, ShadowError, sdsm::*}, SceneView};
//! use worldrunner_render_post::AttachmentView;
//! # fn frame(context: &GpuContext, renderer: &mut ShadowedScene, view: &SceneView,
//! # color: &AttachmentView, depth: &AttachmentView) -> Result<(), ShadowError> {
//! let range = RangeFit { near_step: 0.1, far_step: 1.0, minimum_span: 1.0, split_lambda: 0.5, split_policy: Default::default() };
//! renderer.set_depth_fitting(context, Some(SdsmSettings {
//!     policy: AdoptionPolicy::Interactive { latency_frames: 2, contraction_frames: 4 }, range,
//! }))?;
//! renderer.render(context, view, color, depth)?; // Nonwaiting readback polling.
//! renderer.set_depth_fitting(context, Some(SdsmSettings {
//!     policy: AdoptionPolicy::SameFrameCapture, range,
//! }))?;
//! let fit = renderer.render_depth_fitted(context, view, (color, depth), 42)?;
//! assert_eq!(fit.unwrap().source_frame, 42); // Waits for this frame's reduction.
//! renderer.set_depth_fitting(context, None)?; // Restores the original pass plan.
//! # Ok(())
//! # }
//! ```
mod capture;
mod controller;
pub(super) mod scene;
pub use controller::DepthFitting;
mod reduction;
pub use capture::{CaptureInput, DepthContinuation, DepthPrefix};
pub use reduction::{DepthReducer, PendingReduction};

use std::collections::BTreeMap;
use worldrunner_render_shadow::RangeFilter;
pub use worldrunner_render_shadow::{DepthRange, RangeFit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptionPolicy {
    Interactive {
        latency_frames: u64,
        contraction_frames: u64,
    },
    QuantizedPerBound {
        latency_frames: u64,
        contraction_delay: std::time::Duration,
    },
    SameFrameCapture,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdsmSettings {
    pub policy: AdoptionPolicy,
    pub range: RangeFit,
}
#[derive(Debug, thiserror::Error)]
pub enum SdsmError {
    #[error(transparent)]
    InvalidDepth(#[from] worldrunner_render_shadow::InvalidFit),
    #[error("depth reduction belongs to a replaced resource")]
    StaleResource,
    #[error("depth reduction frame is duplicated or out of order")]
    FrameIdentity,
    #[error(
        "SDSM cascade layer count changed from {allocated} to {requested}; rebuild the shadow renderer"
    )]
    CascadeLayers { allocated: u32, requested: u32 },
    #[error(
        "SDSM source must be sampleable single-sample shared linear metre depth and normal coverage of the same extent"
    )]
    Metadata,
    #[error("depth reduction has not been submitted")]
    NotSubmitted,
    #[error(transparent)]
    Transaction(#[from] worldrunner_render_gpu::submission::TransactionError),
    #[error(transparent)]
    Write(#[from] worldrunner_render_gpu::submission::capture::WriteError),
    #[error("depth reduction mapping failed: {0}")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("depth reduction wait failed: {0}")]
    Poll(#[from] wgpu::PollError),
    #[error("depth reduction mapped range unavailable: {0}")]
    MappedRange(#[from] wgpu::MapRangeError),
    #[error("depth reduction callback was not delivered")]
    Callback,
}
#[derive(Clone, Debug)]
pub struct ReductionTicket {
    pub source_frame: u64,
    generation: u64,
    epoch: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdoptedFit {
    pub source_frame: u64,
    pub adopted_frame: u64,
    pub depth: DepthRange,
}

/// Only completed results enter the queue. Dropping an encoded prefix cannot
/// enqueue evidence. Pending evidence can be invalidated without resetting filters.
#[derive(Debug)]
pub struct AdoptionState {
    settings: SdsmSettings,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    completed: BTreeMap<u64, DepthRange>,
    last_source: Option<u64>,
    filter: RangeFilter,
    per_bound: worldrunner_render_shadow::PerBoundHysteresis,
    started: std::time::Instant,
}
impl AdoptionState {
    pub fn new(settings: SdsmSettings) -> Result<Self, SdsmError> {
        settings.range.validate()?;
        Ok(Self {
            settings,
            generation: std::sync::Arc::default(),
            completed: BTreeMap::new(),
            last_source: None,
            filter: RangeFilter::default(),
            per_bound: worldrunner_render_shadow::PerBoundHysteresis::default(),
            started: std::time::Instant::now(),
        })
    }
    pub fn ticket(&self, source_frame: u64) -> ReductionTicket {
        ReductionTicket {
            source_frame,
            generation: self.generation.load(std::sync::atomic::Ordering::Acquire),
            epoch: self.generation.clone(),
        }
    }
    pub fn invalidate_pending(&mut self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.completed.clear();
    }
    pub fn invalidate(&mut self) {
        self.invalidate_pending();
        self.last_source = None;
        self.filter = RangeFilter::default();
        self.per_bound = worldrunner_render_shadow::PerBoundHysteresis::default();
    }
    pub fn complete(
        &mut self,
        ticket: &ReductionTicket,
        depth: DepthRange,
    ) -> Result<(), SdsmError> {
        ticket.validate()?;
        if !std::sync::Arc::ptr_eq(&ticket.epoch, &self.generation) {
            return Err(SdsmError::StaleResource);
        }
        depth.validate()?;
        if self
            .last_source
            .is_some_and(|last| ticket.source_frame <= last)
        {
            return match self.settings.policy {
                AdoptionPolicy::Interactive { .. } | AdoptionPolicy::QuantizedPerBound { .. } => {
                    Ok(())
                }
                AdoptionPolicy::SameFrameCapture => Err(SdsmError::FrameIdentity),
            };
        }
        if self.completed.contains_key(&ticket.source_frame) {
            return Err(SdsmError::FrameIdentity);
        }
        self.completed.insert(ticket.source_frame, depth);
        Ok(())
    }
    pub fn adopt(&mut self, frame: u64, clip: [f32; 2]) -> Result<Option<AdoptedFit>, SdsmError> {
        self.adopt_at(frame, clip, self.started.elapsed())
    }
    pub fn adopt_at(
        &mut self,
        frame: u64,
        clip: [f32; 2],
        elapsed: std::time::Duration,
    ) -> Result<Option<AdoptedFit>, SdsmError> {
        let source = match self.settings.policy {
            AdoptionPolicy::Interactive { latency_frames, .. }
            | AdoptionPolicy::QuantizedPerBound { latency_frames, .. } => {
                frame.checked_sub(latency_frames).and_then(|eligible| {
                    self.completed
                        .range(..=eligible)
                        .next_back()
                        .map(|(&source, _)| source)
                })
            }
            AdoptionPolicy::SameFrameCapture => Some(frame),
        };
        let Some(source) = source else {
            return Ok(None);
        };
        let Some(depth) = self.completed.remove(&source) else {
            return Ok(None);
        };
        let depth = self.settings.range.fit(depth, clip)?;
        let delay = match self.settings.policy {
            AdoptionPolicy::Interactive {
                contraction_frames, ..
            } => contraction_frames,
            AdoptionPolicy::SameFrameCapture | AdoptionPolicy::QuantizedPerBound { .. } => 0,
        };
        let depth = match self.settings.policy {
            AdoptionPolicy::QuantizedPerBound {
                contraction_delay, ..
            } => self.per_bound.update(depth, elapsed, contraction_delay)?,
            _ => self.filter.update(depth, frame, delay)?,
        };
        self.last_source = Some(source);
        self.completed.retain(|&pending, _| pending > source);
        Ok(Some(AdoptedFit {
            source_frame: source,
            adopted_frame: frame,
            depth,
        }))
    }
}

#[cfg(test)]
#[path = "sdsm_tests.rs"]
mod tests;

impl ReductionTicket {
    pub(super) fn validate(&self) -> Result<(), SdsmError> {
        if self.epoch.load(std::sync::atomic::Ordering::Acquire) != self.generation {
            return Err(SdsmError::StaleResource);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "sdsm_gpu_tests.rs"]
pub(crate) mod gpu_tests;

impl From<&worldrunner_render_plan::SceneDepthFitting> for SdsmSettings {
    fn from(value: &worldrunner_render_plan::SceneDepthFitting) -> Self {
        use worldrunner_render_plan::{SceneDepthAdoption, SceneDepthSplits};
        let (split_lambda, split_policy) = match value.splits {
            SceneDepthSplits::Practical { lambda_micro } => (
                crate::view::meters_scalar(lambda_micro) / 1000.0,
                worldrunner_render_shadow::SplitPolicy::Practical,
            ),
            SceneDepthSplits::OverlapRecurrence {
                multiplier_micro,
                overlap_micro,
            } => (
                0.5,
                worldrunner_render_shadow::SplitPolicy::OverlapRecurrence {
                    multiplier: crate::view::meters_scalar(multiplier_micro) / 1000.0,
                    overlap: crate::view::meters_scalar(overlap_micro) / 1000.0,
                },
            ),
        };
        let policy = match value.adoption {
            SceneDepthAdoption::FrameIndexed {
                latency_frames,
                contraction_frames,
            } => AdoptionPolicy::Interactive {
                latency_frames,
                contraction_frames,
            },
            SceneDepthAdoption::QuantizedPerBound {
                latency_frames,
                contraction_delay_ms,
            } => AdoptionPolicy::QuantizedPerBound {
                latency_frames,
                contraction_delay: std::time::Duration::from_millis(contraction_delay_ms),
            },
        };
        Self {
            policy,
            range: RangeFit {
                near_step: crate::view::meters_scalar(value.near_step_mm),
                far_step: crate::view::meters_scalar(value.far_step_mm),
                minimum_span: crate::view::meters_scalar(value.minimum_span_mm),
                split_lambda,
                split_policy,
            },
        }
    }
}
