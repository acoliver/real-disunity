use super::{RenderScene, Startup, ViewerError};
use std::borrow::Cow;
use worldrunner_render_scene::shadows::DirectionalShadowSettings;

pub(super) fn reduce_backend_census(
    enabled: wgpu::Backends,
    selected: wgpu::Backend,
    observations: impl IntoIterator<Item = (wgpu::Backend, bool)>,
) -> Result<Vec<wgpu::Backend>, ViewerError> {
    if !enabled.contains(selected.into()) {
        return Err(ViewerError::SelectedBackendDisallowed {
            backend: selected,
            enabled,
        });
    }
    let mut presentable = Vec::new();
    for (backend, supported) in observations {
        if enabled.contains(backend.into()) && supported && !presentable.contains(&backend) {
            presentable.push(backend);
        }
    }
    if !presentable.contains(&selected) {
        return Err(ViewerError::SelectedBackendMissing { backend: selected });
    }
    Ok(presentable)
}

pub(super) struct Resolved<'a> {
    pub scene: Cow<'a, RenderScene>,
    pub shadows: Option<DirectionalShadowSettings>,
    pub profile: crate::FrameProfile,
    pub startup: Startup,
}

pub(super) fn resolve_retained(
    host: &mut crate::RetainedHost,
    identity: &crate::AdapterIdentity,
    extent: [u32; 2],
    display: Option<crate::DisplayInfo>,
    census: Option<&[wgpu::Backend]>,
) -> Result<(), ViewerError> {
    host.resolve_initial_content(crate::InitialContext {
        adapter: identity,
        surface_extent: extent,
        display,
        presentable_backends: census,
    })
}

pub(super) fn resolve<'a>(
    host: &'a mut crate::RetainedHost,
    adapter: &wgpu::Adapter,
    surface_extent: [u32; 2],
    display: Option<crate::DisplayInfo>,
    presentable_backends: Option<&[wgpu::Backend]>,
) -> Result<Resolved<'a>, ViewerError> {
    let identity = adapter.get_info().into();
    resolve_retained(
        host,
        &identity,
        surface_extent,
        display,
        presentable_backends,
    )?;
    let mut startup = Startup::from(&host.options);
    startup.recipes = true;
    startup.capture_pool = host.capture_pool;
    let resolved = Resolved {
        scene: Cow::Borrowed(host.initial_scene()),
        shadows: host.options.shadows,
        profile: host.options.frame_profile,
        startup,
    };
    if let Some(shadows) = resolved.shadows {
        shadows.validate(&adapter.limits())?;
    }
    Ok(resolved)
}
