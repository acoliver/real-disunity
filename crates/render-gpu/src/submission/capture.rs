//! Exclusive two-segment captures with owned, read-only frame inputs.
//! Hosts must keep polling the device and call `poll_captures` or `drain_captures`
//! during shutdown, even when no new frame is requested.
mod contracts;
mod lifecycle;
mod service;
pub(super) use contracts::WriteScope;
pub use contracts::{CaptureWrites, WriteError, WriteKey};
pub(super) use service::Cleanup;

use super::{PreparedFrame, SubmissionAuthority, SubmitError};
use std::{
    any::Any,
    sync::{Arc, Mutex},
};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError<E> {
    #[error(transparent)]
    Submission(#[from] SubmitError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error("capture readback failed: {0}")]
    Readback(E),
    #[error("capture preparation failed: {0}")]
    Preparation(E),
    #[error("capture completion wait failed: {0}")]
    Poll(#[from] wgpu::PollError),
}

/// Terminal outcome, including failures observed only through a dropped handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    Complete,
    Cancelled,
    Dropped,
    ReadbackFailed,
    PreparationFailed,
    InvalidWrites,
    DeviceLost,
    SubmissionFailed,
    PollFailed,
}
#[derive(Clone, Debug, Default)]
pub struct CaptureObserver(Arc<Mutex<Option<CaptureOutcome>>>);
impl CaptureObserver {
    pub fn outcome(&self) -> Option<CaptureOutcome> {
        *self.0.lock().unwrap()
    }
    fn set(&self, outcome: CaptureOutcome) {
        self.0.lock().unwrap().get_or_insert(outcome);
    }
}

struct Lease {
    authority: SubmissionAuthority,
    frozen: Arc<dyn Any + Send + Sync>,
    last: Option<wgpu::SubmissionIndex>,
    serial: u64,
    observer: CaptureObserver,
    active: Arc<std::sync::atomic::AtomicBool>,
}
impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureLease")
            .field("serial", &self.serial)
            .finish_non_exhaustive()
    }
}
impl Lease {
    fn fail<E>(&self, error: &CaptureError<E>) {
        let outcome = match error {
            CaptureError::Submission(SubmitError::DeviceLost(_)) => CaptureOutcome::DeviceLost,
            CaptureError::Submission(_) => CaptureOutcome::SubmissionFailed,
            CaptureError::Write(_) => CaptureOutcome::InvalidWrites,
            CaptureError::Readback(_) => CaptureOutcome::ReadbackFailed,
            CaptureError::Preparation(_) => CaptureOutcome::PreparationFailed,
            CaptureError::Poll(_) => CaptureOutcome::PollFailed,
        };
        self.observer.set(outcome);
    }
    fn wait<E>(&self, device: &wgpu::Device) -> Result<(), CaptureError<E>> {
        self.authority.check_device()?;
        device.poll(wgpu::PollType::Wait {
            submission_index: self.last.clone(),
            timeout: Some(std::time::Duration::from_secs(30)),
        })?;
        self.authority.check_device()?;
        Ok(())
    }
    fn submitted(&mut self, submission: wgpu::SubmissionIndex) {
        self.last = Some(submission);
        self.serial = self.authority.submitted_serial();
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let lost = self.authority.check_device().is_err();
        self.observer.set(if lost {
            CaptureOutcome::DeviceLost
        } else if self.last.is_some() {
            CaptureOutcome::Dropped
        } else {
            CaptureOutcome::Cancelled
        });
        let mut state = self.authority.0.state.lock().unwrap();
        if self.last.is_some() && !lost {
            state.capture_cleanup = Some(Cleanup {
                submission: self.last.take().unwrap(),
                serial: self.serial,
                _frozen: self.frozen.clone(),
                active: self.active.clone(),
            });
        } else {
            self.active
                .store(false, std::sync::atomic::Ordering::Release);
            state.preparing = false;
        }
    }
}

/// Prefix inputs are owned for the entire capture, not borrowed from mutable host state.
#[derive(Debug)]
#[must_use = "submit or drop the prefix"]
pub struct PreparedPrefix<F> {
    frame: PreparedFrame,
    lease: Lease,
    frozen: Arc<F>,
    writes: CaptureWrites,
}
/// A continuation belongs to exactly one frame and can be resumed only once.
/// ```compile_fail
/// use worldrunner_render_gpu::submission::capture::CaptureContinuation;
/// fn twice(next: CaptureContinuation<()>, device: &wgpu::Device) {
///     let _ = next.resume(device, |_, _| Ok::<_, ()>(()), |_, _, (), ()| Ok(()));
///     let _ = next.resume(device, |_, _| Ok::<_, ()>(()), |_, _, (), ()| Ok(()));
/// }
/// ```
#[derive(Debug)]
#[must_use = "resume or drop the continuation"]
pub struct CaptureContinuation<F> {
    lease: Lease,
    frozen: Arc<F>,
    writes: CaptureWrites,
}
#[derive(Debug)]
#[must_use = "submit or drop the suffix"]
pub struct PreparedSuffix<F> {
    frame: PreparedFrame,
    lease: Lease,
    frozen: Arc<F>,
}
#[derive(Debug)]
#[must_use = "finish the capture readback or drop it"]
pub struct SubmittedCapture<F> {
    lease: Lease,
    _frozen: Arc<F>,
}
