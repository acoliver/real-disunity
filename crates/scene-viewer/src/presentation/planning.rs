use super::{Extents, capture_allocations, recipe_allocations, setup};
use crate::{RetainedCaptureRecipe, ViewerError, ViewerOptions};
use worldrunner_render_gpu::configuration::{AllocationSet, ConfigurationError};
use worldrunner_render_plan::SceneEffects;
use worldrunner_render_scene::{Viewport, compositor::RendererConfiguration};

/// Capture storage and raster footprint to include in a descriptor-only plan.
#[derive(Clone, Copy, Debug)]
pub enum PresentationCapture<'a> {
    /// Ordinary desktop or live presentation, without a retained recipe pool.
    Ordinary,
    /// Retained presentation, including the configured shared recipe pool.
    Retained,
    /// Retained recipe execution, including supersampling and native RCAS.
    Recipe(&'a RetainedCaptureRecipe),
}

/// Capture mode and history retained before constructing the planned generation.
#[derive(Clone, Copy, Debug)]
pub struct PresentationPlanning<'a> {
    pub capture: PresentationCapture<'a>,
    /// False at startup; otherwise use `ConfiguredFrameRenderer::has_effect_history`.
    /// History remains present after effects are disabled or a recipe is restored.
    pub retained_effect_history: bool,
}

/// Plans one generation without a device or backing allocations. `size` is the
/// actual physical surface extent; ordinary attachments keep the 4096 bound.
/// Pass effective scene effects and the selected device's features and limits.
/// The returned inventory excludes other live generations; replacement admission
/// adds those and deduplicates shared storage before deriving the budget.
pub fn plan_presentation(
    size: [u32; 2],
    options: &ViewerOptions,
    effects: Option<&SceneEffects>,
    limits: &wgpu::Limits,
    features: wgpu::Features,
    planning: PresentationPlanning<'_>,
) -> Result<AllocationSet, ViewerError> {
    let capture = planning.capture;
    let extents = Extents::new(size, limits.max_texture_dimension_2d)?
        .ok_or(ViewerError::InvalidOptions("cannot plan an empty surface"))?;
    let (mut settings, _) = setup::configuration(
        features,
        super::COLOR,
        extents.render,
        options.frame_profile,
        effects,
    )?;
    options.capture_pool.validate()?;
    let policy = super::PresentationPolicy {
        budget: options.presentation_budget,
        recipes: !matches!(capture, PresentationCapture::Ordinary),
        capture_pool: options.capture_pool.clone(),
    };
    let pool = policy.pool_bytes(&settings)?;
    let capture_size = recipe_settings(
        size,
        &mut settings,
        capture,
        limits,
        &options.capture_pool,
        pool.unwrap_or(0),
    )?
    .unwrap_or(extents.surface);
    settings.additional = additional(capture_size, pool);
    let allocations = settings.allocations_with_history(planning.retained_effect_history);
    allocations
        .validate(limits)
        .map_err(ConfigurationError::Invalid)?;
    Ok(allocations)
}

fn recipe_settings(
    size: [u32; 2],
    settings: &mut RendererConfiguration,
    capture: PresentationCapture<'_>,
    limits: &wgpu::Limits,
    pool_policy: &crate::CapturePoolPolicy,
    pool: u64,
) -> Result<Option<Viewport>, ViewerError> {
    let PresentationCapture::Recipe(recipe) = capture else {
        return Ok(None);
    };
    let mut recipe = recipe.clone();
    recipe.set_ssao(settings.ssao);
    recipe.frame_samples = settings.frame.samples;
    pool_policy.reservation_bytes(&recipe, size, pool, 0)?;
    if !recipe.accumulated() {
        return Ok(None);
    }
    let size = recipe
        .render_size(size)
        .map_err(|error| ViewerError::Driver(error.to_string()))?;
    let extents = Extents::new(size, limits.max_texture_dimension_2d)?
        .expect("validated nonzero recipe extent");
    settings.frame.size = size;
    settings.frame.output_size = None;
    settings.post.native_rcas = recipe.native_rcas().or(settings.post.native_rcas);
    Ok(Some(extents.surface))
}

pub(super) fn additional(size: Viewport, pool: Option<u64>) -> AllocationSet {
    let mut allocations = capture_allocations(size);
    if let Some(bytes) = pool {
        allocations.extend(recipe_allocations(bytes));
    }
    allocations
}
