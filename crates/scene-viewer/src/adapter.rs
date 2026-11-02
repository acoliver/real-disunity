//! Caller-owned frame decisions for the adapter selected at startup.
use std::sync::Arc;

pub use worldrunner_render_scene::device::{AdapterClass, AdapterIdentity};

/// CPU frame settings already owned by `ViewerOptions`.
/// Samples and render scale are carried by `frame_profile`; the budget policy
/// sizes presentation storage without choosing an engine quality preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSettings {
    pub frame_profile: crate::FrameProfile,
    pub presentation_budget: crate::PresentationBudgetPolicy,
}

/// Called once at renderer startup, on its calling thread, before device creation
/// or scene preparation. The identity belongs to the adapter used for the device,
/// including surface compatibility in desktop mode. No second adapter is selected.
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
        resolved.frame_profile = settings.frame_profile;
        resolved.presentation_budget = settings.presentation_budget;
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
    retained: Option<(
        &mut u64,
        &mut crate::retained::Captures,
        &crate::retained::readiness::Readiness,
    )>,
) -> Result<(), crate::ViewerError> {
    let changed = resolve(identity, scene, options)?;
    if changed && let Some((pool, captures, readiness)) = retained {
        for preparation in &readiness.queue {
            if let crate::RetainedUpdate::Settings(effects) = &preparation.update {
                crate::RetainedHost::validate_streamed_effects(effects, options)?;
            }
        }
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
