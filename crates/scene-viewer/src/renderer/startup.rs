use super::{RenderScene, SceneSource, Startup, ViewerError};
use std::borrow::Cow;
use worldrunner_render_scene::shadows::DirectionalShadowSettings;

pub(super) struct Resolved<'a> {
    pub scene: Cow<'a, RenderScene>,
    pub shadows: Option<DirectionalShadowSettings>,
    pub profile: crate::FrameProfile,
    pub startup: Startup,
}

pub(super) fn resolve<'a>(
    source: SceneSource<'a>,
    adapter: &wgpu::Adapter,
    surface_extent: [u32; 2],
) -> Result<Resolved<'a>, ViewerError> {
    let identity = adapter.get_info().into();
    let resolved = match source {
        SceneSource::Static { scene, options } => {
            crate::adapter::resolve_startup(&identity, scene, options, None)?;
            Resolved {
                scene: Cow::Borrowed(scene),
                shadows: options.shadows,
                profile: options.frame_profile,
                startup: Startup::from(&*options),
            }
        }
        SceneSource::Retained(host) => {
            host.resolve_initial_content(crate::InitialContext {
                adapter: &identity,
                surface_extent,
            })?;
            let mut startup = Startup::from(&host.options);
            startup.recipes = true;
            startup.capture_pool = host.capture_pool;
            Resolved {
                scene: Cow::Borrowed(host.initial_scene()),
                shadows: host.options.shadows,
                profile: host.options.frame_profile,
                startup,
            }
        }
        SceneSource::Live => Resolved {
            scene: Cow::Owned(super::live::empty_scene()),
            shadows: None,
            profile: crate::FrameProfile::Legacy,
            startup: 2.into(),
        },
    };
    if let Some(shadows) = resolved.shadows {
        shadows.validate(&adapter.limits())?;
    }
    Ok(resolved)
}
