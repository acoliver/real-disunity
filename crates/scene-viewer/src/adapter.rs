//! Caller-owned frame decisions for the adapter selected at startup.
use std::sync::Arc;

pub use worldrunner_render_gpu::adapter_memory::{AdapterMemory, AdapterMemorySource};
pub use worldrunner_render_scene::device::{AdapterClass, AdapterIdentity};

/// CPU frame settings already owned by `ViewerOptions`.
/// Samples and render scale are carried by `frame_profile`; the budget policy
/// sizes presentation storage without choosing an engine quality preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSettings {
    pub shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
    pub frame_profile: crate::FrameProfile,
    pub presentation_budget: crate::PresentationBudgetPolicy,
}

/// Runs once on the calling thread if startup reaches frame settings resolution,
/// after desktop surface compatibility and backend census checks. This precedes
/// GPU scene preparation. Owned paths create a device afterward; shared paths use
/// an existing device. The identity is for the adapter used by that device.
/// No second adapter is selected.
/// `Ok(None)` preserves the supplied options. Errors stop startup as `ViewerError::Driver`.
#[derive(Clone)]
pub struct AdapterCallback(Arc<dyn FrameDecision>);

trait FrameDecision: Send + Sync {
    fn settings(&self, identity: &AdapterIdentity) -> Result<Option<FrameSettings>, String>;
}

impl<F> FrameDecision for F
where
    F: Fn(&AdapterIdentity) -> Result<Option<FrameSettings>, String> + Send + Sync,
{
    fn settings(&self, identity: &AdapterIdentity) -> Result<Option<FrameSettings>, String> {
        self(identity)
    }
}

impl AdapterCallback {
    pub fn new(
        observe: impl Fn(&AdapterIdentity) -> Result<Option<FrameSettings>, String>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(observe))
    }
}

pub(crate) fn resolve(
    identity: &AdapterIdentity,
    scene: &worldrunner_render_plan::RenderScene,
    options: &mut crate::ViewerOptions,
) -> Result<bool, crate::ViewerError> {
    if let Some(callback) = &options.adapter_callback
        && let Some(settings) = callback
            .0
            .settings(identity)
            .map_err(crate::ViewerError::Driver)?
    {
        let mut resolved = options.clone();
        apply_settings(&mut resolved, settings);
        crate::validate(scene, &resolved)?;
        *options = resolved;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn resolve_startup(
    identity: &AdapterIdentity,
    scene: &worldrunner_render_plan::RenderScene,
    options: &mut crate::ViewerOptions,
    retained: Option<(&mut u64, &mut crate::retained::Captures)>,
) -> Result<(), crate::ViewerError> {
    let changed = resolve(identity, scene, options)?;
    if changed && let Some((pool, captures)) = retained {
        *pool = options.capture_pool.effective_bytes(
            options.frame_profile.target().1,
            crate::retained::ao_format_from_settings(
                scene.effects().and_then(|e| e.ssao()).copied(),
            ),
        )?;
        captures.resolve_frame(options, *pool)?;
    }
    Ok(())
}

impl std::fmt::Debug for AdapterCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AdapterCallback")
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod content_tests;

pub(crate) fn apply_settings(options: &mut crate::ViewerOptions, settings: FrameSettings) {
    options.frame_profile = settings.frame_profile;
    options.presentation_budget = settings.presentation_budget;
    options.shadows = settings.shadows;
}

/// Inputs available before the initial retained scene is prepared or made resident.
#[derive(Clone, Copy, Debug)]
pub struct InitialContext<'a> {
    pub adapter: &'a AdapterIdentity,
    /// Physical pixels at surface creation, or the configured windowless extent.
    /// A zero dimension preserves the desktop surface's suspended state.
    pub surface_extent: [u32; 2],
    /// The monitor the window opened on, for choosing render scale from output pixels.
    /// `None` for windowless hosts and when the platform cannot identify the monitor.
    pub display: Option<crate::DisplayInfo>,
    /// Distinct backends in the order their first enumerated adapter supporting this
    /// surface was observed, under the effective instance backend mask.
    /// Desktop startup supplies `Some` even without monitor information. `None` means
    /// unknown, including windowless startup. The census guarantees neither device
    /// creation nor feature support.
    pub presentable_backends: Option<&'a [wgpu::Backend]>,
}

/// Initial scene and optional options chosen on the selected rendering adapter.
pub struct InitialContent {
    pub scene: worldrunner_render_plan::RenderScene,
    pub settings: Option<FrameSettings>,
}
