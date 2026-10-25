use std::{marker::PhantomData, rc::Rc, sync::Arc};
use winit::window::Window;
use worldrunner_render_gpu::{
    flight::{Admission, AdmissionError, FrameAdmission},
    surface::{AcquiredFrame, Presenter},
};

use super::{GpuContext, ViewerError, surface_error};

pub(super) struct ReadyFrame {
    pub admission: Admission,
    pub frame: wgpu::SurfaceTexture,
}

/// Surface and window operations stay on the event-loop thread on every platform.
/// Completion progress uses nonblocking polling; there is no worker to join on shutdown.
pub(super) struct SurfaceExecution {
    pub(super) presenter: Presenter<'static>,
    instance: wgpu::Instance,
    window: Arc<Window>,
    flight: FrameAdmission,
    #[cfg(all(test, target_os = "linux"))]
    pub(super) acquisitions: std::collections::VecDeque<wgpu::CurrentSurfaceTexture>,
    _event_thread: PhantomData<Rc<()>>,
}

impl SurfaceExecution {
    pub fn new(
        presenter: Presenter<'static>,
        instance: wgpu::Instance,
        window: Arc<Window>,
        capacity: usize,
    ) -> Result<Self, ViewerError> {
        Ok(Self {
            presenter,
            instance,
            window,
            flight: FrameAdmission::new(capacity)
                .map_err(|error| ViewerError::Gpu(error.to_string()))?,
            #[cfg(all(test, target_os = "linux"))]
            acquisitions: std::collections::VecDeque::new(),
            _event_thread: PhantomData,
        })
    }

    pub fn submit_prepared(
        &self,
        ready: ReadyFrame,
        prepared: worldrunner_render_gpu::submission::PreparedFrame,
        context: &GpuContext,
    ) -> Result<worldrunner_render_gpu::flight::CompletionTicket, ViewerError> {
        let ticket = self
            .flight
            .submit_prepared(ready.admission, prepared)
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.window.pre_present_notify();
        context.queue().present(ready.frame);
        Ok(ticket)
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) -> Result<(), ViewerError> {
        self.presenter.resize(device, size).map_err(surface_error)?;
        Ok(())
    }

    pub fn acquire(
        &mut self,
        context: &GpuContext,
        update: impl FnOnce() -> Result<(), ViewerError>,
    ) -> Result<Option<ReadyFrame>, ViewerError> {
        let _configuration = if self.presenter.has_pending_transition() {
            match context.submission().try_configuration() {
                Ok(guard) => Some(guard),
                Err(worldrunner_render_gpu::submission::SubmitError::Busy) => return Ok(None),
                Err(error) => return Err(ViewerError::Gpu(error.to_string())),
            }
        } else {
            None
        };
        let flight = self.flight.clone();
        let mut surface = NativeSurface {
            execution: self,
            context,
        };
        acquire_frame(&mut surface, &flight, update)
            .map(|ready| ready.map(|(admission, frame)| ReadyFrame { admission, frame }))
    }

    #[cfg(all(test, target_os = "linux"))]
    pub(super) fn submit_with_completion(
        &self,
        ready: ReadyFrame,
        commands: wgpu::CommandBuffer,
        context: &GpuContext,
        register: impl FnOnce(Box<dyn FnOnce() + Send>),
    ) -> worldrunner_render_gpu::flight::CompletionTicket {
        let ticket = self.flight.submit_with_completion(
            ready.admission,
            context.submission(),
            [commands],
            register,
        );
        self.window.pre_present_notify();
        context.queue().present(ready.frame);
        ticket
    }
}

trait SurfaceOps {
    type Frame;
    fn suspended(&self) -> bool;
    fn pending(&self) -> bool;
    fn apply(&mut self) -> Result<(), ViewerError>;
    fn acquire(&mut self) -> Result<Option<Self::Frame>, ViewerError>;
}

/// This synchronous boundary propagates acquisition failures without a response channel.
fn acquire_frame<S: SurfaceOps>(
    surface: &mut S,
    flight: &FrameAdmission,
    update: impl FnOnce() -> Result<(), ViewerError>,
) -> Result<Option<(Admission, S::Frame)>, ViewerError> {
    if surface.suspended() {
        return Ok(None);
    }
    if surface.pending() {
        if !flight.is_idle() {
            return Ok(None);
        }
        surface.apply()?;
    }
    let admission = match flight.try_acquire() {
        Ok(admission) => admission,
        Err(AdmissionError::WouldBlock) => return Ok(None),
        Err(error) => return Err(ViewerError::Gpu(error.to_string())),
    };
    let Some(frame) = surface.acquire()? else {
        return Ok(None);
    };
    update()?;
    Ok(Some((admission, frame)))
}

struct NativeSurface<'a> {
    execution: &'a mut SurfaceExecution,
    context: &'a GpuContext,
}
impl SurfaceOps for NativeSurface<'_> {
    type Frame = wgpu::SurfaceTexture;
    fn suspended(&self) -> bool {
        self.execution.presenter.is_suspended()
    }
    fn pending(&self) -> bool {
        self.execution.presenter.has_pending_transition()
    }
    fn apply(&mut self) -> Result<(), ViewerError> {
        let execution = &mut *self.execution;
        if matches!(
            execution
                .presenter
                .apply_pending(self.context.device())
                .map_err(surface_error)?,
            AcquiredFrame::Recreate
        ) {
            let surface = execution
                .instance
                .create_surface(execution.window.clone())?;
            let capabilities = surface.get_capabilities(self.context.adapter());
            execution
                .presenter
                .replace_surface(surface, &capabilities, self.context.device())
                .map_err(surface_error)?;
        }
        Ok(())
    }
    fn acquire(&mut self) -> Result<Option<Self::Frame>, ViewerError> {
        #[cfg(all(test, target_os = "linux"))]
        if let Some(outcome) = self.execution.acquisitions.pop_front() {
            self.execution
                .presenter
                .handle_acquired(self.context.device(), outcome)
                .map_err(surface_error)?;
            return Ok(None);
        }
        match self
            .execution
            .presenter
            .acquire(self.context.device())
            .map_err(surface_error)?
        {
            AcquiredFrame::Frame(frame) => Ok(Some(frame)),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests;
