#[cfg(test)]
#[path = "capture/term_tests.rs"]
mod term_tests;
use super::{RetainedEvent, RetainedHost};
mod recipe;
pub use recipe::{CaptureJitter, CaptureOutput, RetainedCaptureProducts, RetainedCaptureRecipe};
use std::collections::{BTreeMap, VecDeque};
use worldrunner_render_scene::offscreen::RgbaImage;

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedCaptureOutcome {
    Completed {
        frame: u64,
        time: std::time::Duration,
        image: RgbaImage,
    },
    RecipeCompleted {
        frame: u64,
        time: std::time::Duration,
        products: RetainedCaptureProducts,
    },
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CaptureRequestError {
    #[error("invalid capture recipe")]
    InvalidRecipe,
    #[error("capture recipe execution mode is not supported")]
    UnsupportedRecipe,
    #[error("linear HDR capture requires an HDR frame profile")]
    HdrRequired,
    #[error("requested frame does not produce {output:?}")]
    FeatureUnavailable { output: CaptureOutput },
    #[error("capture recipe requires {required} bytes, limit is {limit}")]
    RecipeBudget { required: u64, limit: u64 },
    #[error("capture admission closed or bound {limit} reached")]
    BackPressure { limit: usize },
    #[error("generation {generation} has not been accepted")]
    UnknownGeneration { generation: u64 },
    #[error("generation {generation} was superseded by {ready}")]
    Superseded { generation: u64, ready: u64 },
    #[error("page dependency requires paging for generation {generation}")]
    PagingDisabled { generation: u64 },
    #[error("host is exiting")]
    Exiting,
}

pub(crate) struct Captures {
    pub waiting: VecDeque<(u64, u64, bool)>,
    pub geometry: std::collections::BTreeSet<u64>,
    pub geometry_ready: bool,
    outstanding: BTreeMap<u64, u64>,
    pub(crate) recipes: BTreeMap<u64, RetainedCaptureRecipe>,
    reservations: BTreeMap<u64, u64>,
    pub events: VecDeque<RetainedEvent>,
    limit: usize,
    next: u64,
}
impl Default for Captures {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
            geometry: std::collections::BTreeSet::new(),
            geometry_ready: false,
            outstanding: BTreeMap::new(),
            recipes: BTreeMap::new(),
            reservations: BTreeMap::new(),
            events: VecDeque::new(),
            limit: 64,
            next: 0,
        }
    }
}
impl Captures {
    pub(crate) fn accepts_result(&self, id: u64) -> bool {
        self.outstanding.contains_key(&id) && !self.events.iter().any(|e| matches!(e,
            RetainedEvent::Capture { id: event_id, outcome: RetainedCaptureOutcome::Cancelled, .. } if *event_id == id))
    }

    pub(crate) fn complete(
        &mut self,
        id: u64,
        frame: u64,
        time: std::time::Duration,
        image: RgbaImage,
    ) {
        let generation = self.outstanding[&id];
        self.events.push_back(RetainedEvent::Capture {
            id,
            generation,
            outcome: RetainedCaptureOutcome::Completed { frame, time, image },
        });
    }
    pub(crate) fn resize_reservation(
        &mut self,
        id: u64,
        size: [u32; 2],
    ) -> Result<(), CaptureRequestError> {
        let Some(recipe) = self.recipes.get(&id) else {
            return Ok(());
        };
        let required = recipe.pool_bytes(size)?;
        let other: u64 = self
            .reservations
            .iter()
            .filter(|(key, _)| **key != id)
            .map(|(_, value)| value)
            .sum();
        let limit = RetainedCaptureRecipe::POOL_BYTES;
        if required > limit - other {
            return Err(CaptureRequestError::RecipeBudget {
                required: required + other,
                limit,
            });
        }
        self.reservations.insert(id, required);
        Ok(())
    }
    pub(crate) fn complete_recipe(
        &mut self,
        id: u64,
        frame: u64,
        time: std::time::Duration,
        products: RetainedCaptureProducts,
    ) {
        self.events.push_back(RetainedEvent::Capture {
            id,
            generation: self.outstanding[&id],
            outcome: RetainedCaptureOutcome::RecipeCompleted {
                frame,
                time,
                products,
            },
        });
    }
    pub(crate) fn take_event(&mut self) -> Option<RetainedEvent> {
        let event = self.events.pop_front()?;
        if let RetainedEvent::Capture { id, .. } = &event {
            self.outstanding.remove(id).expect("outstanding capture");
            self.recipes.remove(id);
            self.geometry.remove(id);
            self.reservations.remove(id);
        }
        Some(event)
    }
    pub(crate) fn cancel(&mut self) {
        self.waiting.clear();
        self.geometry.clear();
        self.events.clear();
        for (&id, &generation) in &self.outstanding {
            self.events.push_back(RetainedEvent::Capture {
                id,
                generation,
                outcome: RetainedCaptureOutcome::Cancelled,
            });
        }
    }
    pub(crate) fn holds_generation(&self, generation: u64) -> bool {
        self.waiting.iter().any(|request| request.1 == generation)
    }
}

pub(super) fn features(effects: Option<&worldrunner_render_plan::SceneEffects>) -> [bool; 5] {
    let ao = effects.is_some_and(|e| e.ssao().is_some());
    let ssr = effects.and_then(|e| e.ssr()).is_some_and(|s| s.enabled);
    [ao || ssr, ao || ssr, ao || ssr, ssr, ssr]
}

pub(crate) fn ao_format(
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> wgpu::TextureFormat {
    ao_format_from_settings(effects.and_then(|e| e.ssao()).copied())
}

pub(crate) fn ao_format_from_settings(
    settings: Option<worldrunner_render_plan::SceneSsao>,
) -> wgpu::TextureFormat {
    use worldrunner_render_plan::{SceneSsaoKernel, SceneSsaoReceiver};
    match settings {
        Some(ao) if ao.receiver == SceneSsaoReceiver::DepthAwareGather => {
            wgpu::TextureFormat::Rg32Float
        }
        Some(ao) if matches!(ao.kernel, SceneSsaoKernel::Scalable { .. }) => {
            wgpu::TextureFormat::Rg16Float
        }
        _ => wgpu::TextureFormat::R32Float,
    }
}

impl RetainedHost {
    pub(crate) fn capture_ready(&self, request: &(u64, u64, bool), pages: bool) -> bool {
        request.1 == self.ready_generation()
            && !self.readiness.page_wait
            && !self.readiness.geometry_wait
            && (!request.2 || pages)
            && (!self.captures.geometry.contains(&request.0) || self.captures.geometry_ready)
    }

    fn validate_capture_features(
        &self,
        generation: u64,
        recipe: &RetainedCaptureRecipe,
    ) -> Result<wgpu::TextureFormat, CaptureRequestError> {
        if generation > self.accepted_generation() {
            return Err(CaptureRequestError::UnknownGeneration { generation });
        }
        if generation < self.readiness.published {
            return Err(CaptureRequestError::Superseded {
                generation,
                ready: self.readiness.published,
            });
        }
        let (available, format) = if generation == self.readiness.published {
            (
                self.readiness.capture_features,
                self.readiness.capture_ao_format,
            )
        } else {
            let pending = self
                .readiness
                .queue
                .iter()
                .find(|p| p.generation == generation)
                .expect("accepted generation");
            (pending.capture_features, pending.capture_ao_format)
        };
        for (index, output) in [
            CaptureOutput::Depth,
            CaptureOutput::Normals,
            CaptureOutput::AmbientOcclusion,
            CaptureOutput::SsrConfidence,
            CaptureOutput::Specular,
        ]
        .into_iter()
        .enumerate()
        {
            if recipe.outputs.contains(&output) {
                if !available[index] {
                    return Err(CaptureRequestError::FeatureUnavailable { output });
                }
                if recipe.accumulated() {
                    return Err(CaptureRequestError::UnsupportedRecipe);
                }
            }
        }
        Ok(format)
    }

    /// Admits typed output requests without allocating GPU or image storage.
    pub fn request_capture_recipe(
        &mut self,
        generation: u64,
        mut recipe: RetainedCaptureRecipe,
    ) -> Result<u64, CaptureRequestError> {
        recipe.frame_samples = match self.options.frame_profile {
            crate::FrameProfile::Legacy => 1,
            crate::FrameProfile::Hdr { samples, .. }
            | crate::FrameProfile::HdrNativeRcas { samples, .. }
            | crate::FrameProfile::HdrUpscaled { samples, .. } => samples,
        };
        recipe.ao_format = self.validate_capture_features(generation, &recipe)?;
        let required = recipe.pool_bytes(self.size)?;
        let reserved: u64 = self.captures.reservations.values().sum();
        let limit = RetainedCaptureRecipe::POOL_BYTES;
        if required > limit - reserved {
            return Err(CaptureRequestError::RecipeBudget {
                required: required + reserved,
                limit,
            });
        }
        if recipe.outputs.iter().any(|output| output.term().is_some()) && recipe.accumulated() {
            return Err(CaptureRequestError::UnsupportedRecipe);
        }
        if (recipe.outputs.contains(&CaptureOutput::LinearHdr) || recipe.native_rcas().is_some())
            && matches!(self.options.frame_profile, crate::FrameProfile::Legacy)
        {
            return Err(CaptureRequestError::HdrRequired);
        }
        let id = self.request_capture_with_dependency(generation, recipe.require_pages)?;
        if recipe.require_geometry {
            self.captures.geometry.insert(id);
        }
        self.captures.recipes.insert(id, recipe);
        self.captures.reservations.insert(id, required);
        Ok(id)
    }

    /// Configures the bound before issuing requests. Notifications count until delivered.
    pub fn with_capture_limit(mut self, limit: usize) -> Result<Self, super::ViewerError> {
        if limit == 0 || !self.captures.outstanding.is_empty() {
            return Err(super::ViewerError::InvalidOptions(
                "capture limit must be positive and configured before requests",
            ));
        }
        self.captures.limit = limit;
        Ok(self)
    }

    /// Accepted future generations wait for publication. Unknown and superseded
    /// generations are rejected synchronously, without consuming capacity.
    /// A pending successor closes admission for the ready generation with
    /// `BackPressure`, so its finite accepted cohort drains before publication.
    pub fn request_capture(&mut self, generation: u64) -> Result<u64, CaptureRequestError> {
        self.request_capture_with_dependency(generation, false)
    }

    fn request_capture_with_dependency(
        &mut self,
        generation: u64,
        require_pages: bool,
    ) -> Result<u64, CaptureRequestError> {
        if self.exit {
            return Err(CaptureRequestError::Exiting);
        }
        if generation > self.accepted_generation() {
            return Err(CaptureRequestError::UnknownGeneration { generation });
        }
        if generation < self.readiness.published {
            return Err(CaptureRequestError::Superseded {
                generation,
                ready: self.readiness.published,
            });
        }
        if require_pages && !self.readiness.pages_enabled_for(generation) {
            return Err(CaptureRequestError::PagingDisabled { generation });
        }
        if (generation == self.ready_generation() && !self.readiness.queue.is_empty())
            || self.captures.outstanding.len() == self.captures.limit
        {
            return Err(CaptureRequestError::BackPressure {
                limit: self.captures.limit,
            });
        }
        let id = self.captures.next;
        self.captures.next = id.checked_add(1).expect("capture ID overflow");
        self.captures.outstanding.insert(id, generation);
        self.captures
            .waiting
            .push_back((id, generation, require_pages));
        Ok(id)
    }

    /// Captures after the current view meets the terrain geometry sampling threshold.
    /// Insufficient residency budgets keep the request pending rather than weakening it.
    pub fn request_capture_with_geometry(
        &mut self,
        generation: u64,
    ) -> Result<u64, CaptureRequestError> {
        let id = self.request_capture(generation)?;
        self.captures.geometry.insert(id);
        Ok(id)
    }

    /// Captures the first frame whose current-view selected material pages are resident.
    /// Disabled paging is rejected without consuming request capacity.
    pub fn request_capture_with_pages(
        &mut self,
        generation: u64,
    ) -> Result<u64, CaptureRequestError> {
        self.request_capture_with_dependency(generation, true)
    }
}

#[cfg(test)]
mod tests;
