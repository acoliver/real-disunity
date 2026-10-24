use super::{
    CaptureContinuation, CaptureError, CaptureObserver, CaptureOutcome, Lease, PreparedPrefix,
    PreparedSuffix, SubmittedCapture, service,
};
use crate::submission::Publications;

impl<F> PreparedPrefix<F> {
    pub fn observer(&self) -> CaptureObserver {
        self.lease.observer.clone()
    }
    pub fn submit(
        mut self,
    ) -> Result<CaptureContinuation<F>, CaptureError<std::convert::Infallible>> {
        if let Err(error) = service::submit(&mut self.frame, &mut self.lease) {
            self.lease.fail(&error);
            return Err(error);
        }
        Ok(CaptureContinuation {
            lease: self.lease,
            frozen: self.frozen,
            writes: self.writes,
        })
    }
}
impl<F> CaptureContinuation<F> {
    pub fn observer(&self) -> CaptureObserver {
        self.lease.observer.clone()
    }
    /// Waits for this prefix, then obtains readback evidence before preparing suffix inputs.
    /// The consuming handle prevents a second resume or a foreign frame identity.
    /// The initial wait cannot service mapping registered afterward by `readback`.
    /// That callback must poll the device itself or arrange another poller.
    pub fn resume<R, E>(
        self,
        device: &wgpu::Device,
        readback: impl FnOnce(&wgpu::Device, wgpu::SubmissionIndex) -> Result<R, E>,
        prepare: impl FnOnce(&mut wgpu::CommandEncoder, &mut Publications, &F, &R) -> Result<(), E>,
    ) -> Result<PreparedSuffix<F>, CaptureError<E>> {
        let result = self.prepare_suffix(device, readback, prepare);
        match result {
            Ok(suffix) => Ok(suffix),
            Err((lease, error)) => {
                lease.fail(&error);
                Err(error)
            }
        }
    }
    fn prepare_suffix<R, E>(
        self,
        device: &wgpu::Device,
        readback: impl FnOnce(&wgpu::Device, wgpu::SubmissionIndex) -> Result<R, E>,
        prepare: impl FnOnce(&mut wgpu::CommandEncoder, &mut Publications, &F, &R) -> Result<(), E>,
    ) -> Result<PreparedSuffix<F>, (Lease, CaptureError<E>)> {
        let result = (|| {
            self.lease.wait(device)?;
            let data = readback(device, self.lease.last.clone().unwrap());
            self.lease.authority.check_device()?;
            let data = data.map_err(CaptureError::Readback)?;
            service::record(
                &self.lease.authority,
                device,
                self.writes.suffix,
                self.lease.active.clone(),
                |encoder, edits| prepare(encoder, edits, &self.frozen, &data),
            )
        })();
        match result {
            Ok(frame) => Ok(PreparedSuffix {
                frame,
                lease: self.lease,
                frozen: self.frozen,
            }),
            Err(error) => Err((self.lease, error)),
        }
    }
}
impl<F> PreparedSuffix<F> {
    pub fn observer(&self) -> CaptureObserver {
        self.lease.observer.clone()
    }
    pub fn submit(mut self) -> Result<SubmittedCapture<F>, CaptureError<std::convert::Infallible>> {
        if let Err(error) = service::submit(&mut self.frame, &mut self.lease) {
            self.lease.fail(&error);
            return Err(error);
        }
        Ok(SubmittedCapture {
            lease: self.lease,
            _frozen: self.frozen,
        })
    }
}
impl<F> SubmittedCapture<F> {
    pub fn observer(&self) -> CaptureObserver {
        self.lease.observer.clone()
    }
    /// No successful capture receipt is published until suffix completion and final readback.
    /// The initial wait cannot service mapping registered afterward by `readback`.
    /// That callback must poll the device itself or arrange another poller.
    pub fn finish<R, E>(
        self,
        device: &wgpu::Device,
        readback: impl FnOnce(&wgpu::Device, wgpu::SubmissionIndex) -> Result<R, E>,
    ) -> Result<R, CaptureError<E>> {
        let result = (|| {
            self.lease.wait(device)?;
            let output = readback(device, self.lease.last.clone().unwrap());
            self.lease.authority.check_device()?;
            output.map_err(CaptureError::Readback)
        })();
        match &result {
            Ok(_) => self.lease.observer.set(CaptureOutcome::Complete),
            Err(error) => self.lease.fail(error),
        }
        let authority = self.lease.authority.clone();
        drop(self);
        authority.retire_capture();
        result
    }
}
