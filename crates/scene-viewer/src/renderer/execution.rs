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
            _event_thread: PhantomData,
        })
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
        let flight = self.flight.clone();
        let mut surface = NativeSurface {
            execution: self,
            context,
        };
        acquire_frame(&mut surface, &flight, update)
            .map(|ready| ready.map(|(admission, frame)| ReadyFrame { admission, frame }))
    }

    pub fn submit(&self, ready: ReadyFrame, commands: wgpu::CommandBuffer, context: &GpuContext) {
        self.submit_with_completion(ready, commands, context, |complete| {
            context.queue().on_submitted_work_done(complete);
        });
    }

    pub(super) fn submit_with_completion(
        &self,
        ready: ReadyFrame,
        commands: wgpu::CommandBuffer,
        context: &GpuContext,
        register: impl FnOnce(Box<dyn FnOnce() + Send>),
    ) -> worldrunner_render_gpu::flight::CompletionTicket {
        let ticket = self.flight.submit_with_completion(
            ready.admission,
            context.queue(),
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
        #[cfg(test)]
        trace_skip("suspended");
        return Ok(None);
    }
    if surface.pending() {
        if !flight.is_idle() {
            #[cfg(test)]
            trace_skip("pending transition waiting for flight");
            return Ok(None);
        }
        surface.apply()?;
    }
    let admission = match flight.try_acquire() {
        Ok(admission) => admission,
        Err(AdmissionError::WouldBlock) => {
            #[cfg(test)]
            trace_skip("flight capacity");
            return Ok(None);
        }
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
        match self
            .execution
            .presenter
            .acquire(self.context.device())
            .map_err(surface_error)?
        {
            AcquiredFrame::Frame(frame) => Ok(Some(frame)),
            _other => {
                #[cfg(test)]
                trace_skip(&format!("native {_other:?}"));
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
fn trace_skip(reason: &str) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SKIPS: AtomicUsize = AtomicUsize::new(0);
    let count = SKIPS.fetch_add(1, Ordering::Relaxed) + 1;
    if count <= 32 || count.is_power_of_two() {
        eprintln!("acquire skip {count}: {reason}");
    }
}

#[cfg(test)]
mod tests;
