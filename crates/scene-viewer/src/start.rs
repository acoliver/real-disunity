//! A window start that makes its GPU device before the window exists, so another user, such as
//! a player's offscreen work, can run on the same device and submission order from the start.
//!
//! [`ViewerStart::new`] makes the event loop and the instance on its display. [`ViewerStart::device`]
//! makes the device for the viewer's needs combined with the caller's, before the scene loads.
//! [`crate::RetainedHost::run_on`] then opens the window on that device. Its surface is checked
//! against the adapter when the window exists: an adapter that cannot present to it is refused
//! by name, as is a scene that needs a feature the device lacks.
use winit::event_loop::EventLoop;
use worldrunner_render_gpu::device::DeviceRequirements;
use worldrunner_render_gpu::submission::SubmissionAuthority;

use crate::app::UserEvent;
use crate::renderer::Shared;
use crate::{ViewerError, ViewerOptions};

/// The instance, adapter and device a viewer draws on, made before its window or session so
/// another user can share them. [`ViewerStart`] holds one for a window; [`SharedGpu::headless`]
/// makes one for [`crate::RetainedHost::windowless_on`].
pub struct SharedGpu {
    instance: wgpu::Instance,
    adapter: Option<wgpu::Adapter>,
    context: Option<worldrunner_render_scene::device::GpuContext>,
}

impl SharedGpu {
    fn on(instance: wgpu::Instance) -> Self {
        Self {
            instance,
            adapter: None,
            context: None,
        }
    }

    /// An instance with no display, for a session with no window.
    #[must_use]
    pub fn headless() -> Self {
        Self::on(wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
        ))
    }

    /// The adapter the device will be made on. The first call chooses it, without a surface, and
    /// later calls return the same one. No device exists until [`Self::device`], so a caller can
    /// size its requirements to what the adapter offers.
    ///
    /// # Errors
    /// Fails when there is no adapter.
    pub fn adapter(&mut self) -> Result<wgpu::Adapter, ViewerError> {
        if self.adapter.is_none() {
            self.adapter = Some(pollster::block_on(
                self.instance
                    .request_adapter(&wgpu::RequestAdapterOptions::default()),
            )?);
        }
        Ok(self.adapter.clone().expect("made above"))
    }

    /// The device, made once for the viewer's needs under `options` combined with `caller`'s, with
    /// its submission authority. A later call returns the same device.
    ///
    /// # Errors
    /// Fails when there is no adapter or it cannot meet either side's required features or limits.
    pub fn device(
        &mut self,
        options: &ViewerOptions,
        caller: &DeviceRequirements,
    ) -> Result<(wgpu::Device, SubmissionAuthority), ViewerError> {
        if self.context.is_none() {
            let adapter = self.adapter()?;
            self.context = Some(pollster::block_on(
                crate::renderer::timing::shared_context(adapter, options.gpu_timing, caller),
            )?);
        }
        let context = self.context.as_ref().expect("made above");
        Ok((context.device().clone(), context.submission().clone()))
    }

    /// The instance and device, if [`Self::device`] made one.
    pub(crate) fn into_shared(self) -> Option<Shared> {
        let instance = self.instance;
        self.context.map(|context| Shared { instance, context })
    }
}

/// The event loop a window will run on, and the device made for it ahead of the window.
pub struct ViewerStart {
    pub(crate) event_loop: EventLoop<UserEvent>,
    gpu: SharedGpu,
}

impl ViewerStart {
    /// The event loop and the instance on its display. Call on the main thread, once per
    /// process, in place of the event loop [`crate::RetainedHost::run`] would make.
    ///
    /// # Errors
    /// Fails when the platform has no event loop.
    pub fn new() -> Result<Self, ViewerError> {
        let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            ));
        Ok(Self {
            event_loop,
            gpu: SharedGpu::on(instance),
        })
    }

    /// As [`SharedGpu::adapter`].
    ///
    /// # Errors
    /// Fails when there is no adapter.
    pub fn adapter(&mut self) -> Result<wgpu::Adapter, ViewerError> {
        self.gpu.adapter()
    }

    /// As [`SharedGpu::device`]: the device the window will draw on, made once for the viewer's
    /// needs under `options` combined with `caller`'s, before the scene loads. No surface exists
    /// yet, so the adapter is chosen without one.
    ///
    /// # Errors
    /// Fails when there is no adapter or it cannot meet either side's required features or limits.
    pub fn device(
        &mut self,
        options: &ViewerOptions,
        caller: &DeviceRequirements,
    ) -> Result<(wgpu::Device, SubmissionAuthority), ViewerError> {
        self.gpu.device(options, caller)
    }

    /// The event loop, and the device to open the window on if [`Self::device`] made one.
    pub(crate) fn into_parts(self) -> (EventLoop<UserEvent>, Option<Shared>) {
        (self.event_loop, self.gpu.into_shared())
    }
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
