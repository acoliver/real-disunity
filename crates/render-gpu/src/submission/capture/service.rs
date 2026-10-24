use super::{
    Any, Arc, CaptureError, CaptureObserver, CaptureWrites, Lease, PreparedFrame, PreparedPrefix,
    SubmissionAuthority, SubmitError, WriteKey, WriteScope,
};
use crate::submission::Publications;
use std::{collections::BTreeSet, rc::Rc};

pub(in crate::submission) struct Cleanup {
    pub(super) submission: wgpu::SubmissionIndex,
    pub(super) serial: u64,
    pub(super) _frozen: Arc<dyn Any + Send + Sync>,
    pub(super) active: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        self.active
            .store(false, std::sync::atomic::Ordering::Release);
    }
}
impl std::fmt::Debug for Cleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureCleanup")
            .field("serial", &self.serial)
            .finish_non_exhaustive()
    }
}
impl SubmissionAuthority {
    /// Poll this during idle/event-loop iterations to retire dropped captures.
    pub fn poll_captures(
        &self,
        device: &wgpu::Device,
    ) -> Result<(), CaptureError<std::convert::Infallible>> {
        device.poll(wgpu::PollType::Poll)?;
        self.retire_capture();
        self.check_device()?;
        Ok(())
    }
    /// Shutdown waits for submitted capture work, including an abandoned prefix.
    /// A still-owned continuation remains exclusive and returns Busy.
    pub fn drain_captures(
        &self,
        device: &wgpu::Device,
    ) -> Result<(), CaptureError<std::convert::Infallible>> {
        let submission = {
            let state = self.0.state.lock().unwrap();
            state
                .capture_cleanup
                .as_ref()
                .map(|cleanup| cleanup.submission.clone())
        };
        if let Some(submission) = submission
            && self.check_device().is_ok()
        {
            device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })?;
        }
        self.retire_capture();
        self.check_device()?;
        if self.0.state.lock().unwrap().preparing {
            return Err(SubmitError::Busy.into());
        }
        Ok(())
    }
    pub(in crate::submission) fn retire_capture(&self) {
        let lost = self.check_device().is_err();
        let mut state = self.0.state.lock().unwrap();
        if state
            .capture_cleanup
            .as_ref()
            .is_some_and(|cleanup| lost || self.completed_serial() >= cleanup.serial)
        {
            state.capture_cleanup = None;
            state.preparing = false;
        }
    }
    pub fn begin_capture<F: Send + Sync + 'static, E>(
        &self,
        device: &wgpu::Device,
        writes: CaptureWrites,
        frozen: F,
        prepare: impl FnOnce(&mut wgpu::CommandEncoder, &mut Publications, &F) -> Result<(), E>,
    ) -> Result<PreparedPrefix<F>, CaptureError<E>> {
        self.retire_capture();
        self.check_device()?;
        {
            let mut state = self.0.state.lock().unwrap();
            if state.preparing
                || state.configuring
                || self.0.in_flight.load(std::sync::atomic::Ordering::Acquire) != 0
            {
                return Err(SubmitError::Busy.into());
            }
            state.preparing = true;
        }
        let frozen = Arc::new(frozen);
        let lease = Lease {
            authority: self.clone(),
            frozen: frozen.clone(),
            last: None,
            serial: 0,
            observer: CaptureObserver::default(),
            active: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        };
        let frame = match record(
            self,
            device,
            writes.prefix.clone(),
            lease.active.clone(),
            |encoder, edits| prepare(encoder, edits, &frozen),
        ) {
            Ok(frame) => frame,
            Err(error) => {
                lease.fail(&error);
                return Err(error);
            }
        };
        Ok(PreparedPrefix {
            frame,
            lease,
            frozen,
            writes,
        })
    }
}
pub(super) fn record<E>(
    authority: &SubmissionAuthority,
    device: &wgpu::Device,
    allowed: BTreeSet<WriteKey>,
    active: Arc<std::sync::atomic::AtomicBool>,
    prepare: impl FnOnce(&mut wgpu::CommandEncoder, &mut Publications) -> Result<(), E>,
) -> Result<PreparedFrame, CaptureError<E>> {
    authority.check_device()?;
    let scope = Rc::new(WriteScope::new(allowed, active));
    let mut publications = Publications {
        scope: Some(scope.clone()),
        ..Publications::default()
    };
    let mut recording = authority.record(device, publications.clone(), false);
    prepare(&mut recording, &mut publications).map_err(CaptureError::Preparation)?;
    scope.result()?;
    authority.check_device()?;
    Ok(recording.finish())
}
pub(super) fn submit<E>(
    frame: &mut PreparedFrame,
    lease: &mut Lease,
) -> Result<(), CaptureError<E>> {
    frame.publications.scope.as_ref().unwrap().result()?;
    let submission = frame.enqueue_commands()?;
    lease.submitted(submission);
    frame.commit();
    Ok(())
}
