//! A window start that makes its GPU device before the window exists, so another user, such as
//! a player's offscreen work, can run on the same device and submission order from the start.
//!
//! [`ViewerStart::new`] makes the event loop and the instance on its display. [`ViewerStart::gpu`]
//! gives the [`SharedGpu`] whose [`SharedGpu::device`] makes the device for the viewer's needs
//! combined with the caller's, before the scene loads.
//! [`crate::RetainedHost::run`] then opens the window on that device. Its surface is checked
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
/// makes one for [`crate::RetainedHost::windowless`].
pub struct SharedGpu {
    instance: wgpu::Instance,
    backends: wgpu::Backends,
    adapter: Option<wgpu::Adapter>,
    context: Option<worldrunner_render_scene::device::GpuContext>,
}

impl SharedGpu {
    fn on(instance: wgpu::Instance, backends: wgpu::Backends) -> Self {
        Self {
            instance,
            backends,
            adapter: None,
            context: None,
        }
    }

    /// An instance with no display, for a session with no window.
    #[must_use]
    pub fn headless() -> Self {
        let descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        let backends = descriptor.backends;
        Self::on(wgpu::Instance::new(descriptor), backends)
    }

    /// [`Self::headless`] on render-gpu's test instance, which loads no `VK_EXT_debug_utils`.
    /// Parallel test devices crashed inside the Vulkan loader naming objects through that
    /// extension (hub issue 12ae16c68a94), and the production instance loads it in debug builds.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn headless_for_test() -> Self {
        let instance = worldrunner_render_gpu::test_support::test_instance();
        // The backends matter only to a window's presentable-backend census, which a headless
        // instance has no surface for.
        Self::on(instance, wgpu::Backends::PRIMARY)
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
        let backends = self.backends;
        self.context.map(|context| Shared {
            instance,
            backends,
            context,
        })
    }
}

/// The event loop a window will run on, and the device made for it ahead of the window.
pub struct ViewerStart {
    pub(crate) event_loop: EventLoop<UserEvent>,
    gpu: SharedGpu,
}

impl ViewerStart {
    /// The event loop and the instance on its display. Call on the main thread, once per
    /// process, for [`crate::RetainedHost::run`].
    ///
    /// # Errors
    /// Fails when the platform has no event loop.
    pub fn new() -> Result<Self, ViewerError> {
        let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
        let descriptor = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
            event_loop.owned_display_handle(),
        ));
        let backends = descriptor.backends;
        let instance = wgpu::Instance::new(descriptor);
        Ok(Self {
            event_loop,
            gpu: SharedGpu::on(instance, backends),
        })
    }

    /// The instance, adapter and device the window will draw on. Make the adapter and the device
    /// through it ([`SharedGpu::adapter`], [`SharedGpu::device`]) before the scene loads, so a
    /// caller can size its requirements to the adapter and share the one device with the viewer.
    /// No surface exists yet, so the adapter is chosen without one.
    pub fn gpu(&mut self) -> &mut SharedGpu {
        &mut self.gpu
    }

    /// The event loop, and the device to open the window on if [`Self::device`] made one.
    pub(crate) fn into_parts(self) -> (EventLoop<UserEvent>, Option<Shared>) {
        (self.event_loop, self.gpu.into_shared())
    }
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;
