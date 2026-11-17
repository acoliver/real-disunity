//! The instance, surface, device and presentation policy a window starts with: made for the
//! window, or adopted from a device made before it, which another user shares.
use std::sync::Arc;

use winit::{event_loop::OwnedDisplayHandle, window::Window};
use worldrunner_render_gpu::surface::SurfacePolicy;
use worldrunner_render_scene::device::{DeviceError, GpuContext};

use super::{Resolved, required_features, surface_policy, timing};
use crate::ViewerError;

/// A device made before the window, such as one a player's offscreen work also runs on, and the
/// instance it came from. The window's surface is made on that instance.
pub(crate) struct Shared {
    pub instance: wgpu::Instance,
    pub context: GpuContext,
}

pub(super) type SurfaceContext<'a> = (
    wgpu::Instance,
    wgpu::Surface<'static>,
    GpuContext,
    SurfacePolicy,
    Resolved<'a>,
);

/// The window's start: on `shared` when given, otherwise on an adapter chosen for the window.
pub(super) async fn surface_context<'a>(
    display: OwnedDisplayHandle,
    window: Arc<Window>,
    shared: Option<Shared>,
    resolve: impl FnOnce(&wgpu::Adapter, [u32; 2]) -> Result<Resolved<'a>, ViewerError>,
) -> Result<SurfaceContext<'a>, ViewerError> {
    match shared {
        Some(shared) => adopt(&window, shared, resolve),
        None => own(display, window, resolve).await,
    }
}

async fn own<'a>(
    display: OwnedDisplayHandle,
    window: Arc<Window>,
    resolve: impl FnOnce(&wgpu::Adapter, [u32; 2]) -> Result<Resolved<'a>, ViewerError>,
) -> Result<SurfaceContext<'a>, ViewerError> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(
        Box::new(display),
    ));
    let surface = instance.create_surface(window.clone())?;
    let size = window.inner_size();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await?;
    let capabilities = surface.get_capabilities(&adapter);
    let resolved = resolve(&adapter, [size.width, size.height])?;
    let startup = &resolved.startup;
    let policy = surface_policy(&capabilities, [size.width, size.height], startup)?;
    let context = timing::context(
        adapter,
        required_features(&resolved.scene),
        startup.gpu_timing,
    )
    .await?;
    Ok((instance, surface, context, policy, resolved))
}

/// The window on `shared`'s device. Its adapter was chosen before the window existed, so the
/// surface is checked here: an adapter that cannot present to it, or a device without the
/// features the scene needs, is refused by name.
fn adopt<'a>(
    window: &Arc<Window>,
    shared: Shared,
    resolve: impl FnOnce(&wgpu::Adapter, [u32; 2]) -> Result<Resolved<'a>, ViewerError>,
) -> Result<SurfaceContext<'a>, ViewerError> {
    let Shared { instance, context } = shared;
    let surface = instance.create_surface(window.clone())?;
    let adapter = context.adapter();
    if !adapter.is_surface_supported(&surface) {
        return Err(ViewerError::SharedSurface {
            adapter: adapter.get_info().name,
        });
    }
    let size = window.inner_size();
    let capabilities = surface.get_capabilities(adapter);
    let resolved = resolve(adapter, [size.width, size.height])?;
    let missing = required_features(&resolved.scene).difference(context.device().features());
    if !missing.is_empty() {
        return Err(DeviceError::MissingFeatures {
            adapter: adapter.get_info().name,
            missing,
        }
        .into());
    }
    let policy = surface_policy(&capabilities, [size.width, size.height], &resolved.startup)?;
    Ok((instance, surface, context, policy, resolved))
}
