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
        let required = recipe.storage_bytes(size)?;
        let other: u64 = self
            .reservations
            .iter()
            .filter(|(key, _)| **key != id)
            .map(|(_, value)| value)
            .sum();
        let limit = 256 * 1024 * 1024;
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
            self.reservations.remove(id);
        }
        Some(event)
    }
    pub(crate) fn cancel(&mut self) {
        self.waiting.clear();
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

impl RetainedHost {
    /// Admits typed output requests without allocating GPU or image storage.
    pub fn request_capture_recipe(
        &mut self,
        generation: u64,
        recipe: RetainedCaptureRecipe,
    ) -> Result<u64, CaptureRequestError> {
        let required = recipe.storage_bytes(self.size)?;
        let reserved: u64 = self.captures.reservations.values().sum();
        let limit = 256 * 1024 * 1024;
        if required > limit - reserved {
            return Err(CaptureRequestError::RecipeBudget {
                required: required + reserved,
                limit,
            });
        }
        if recipe
            .outputs
            .iter()
            .any(|o| !matches!(o, CaptureOutput::Srgb8 | CaptureOutput::LinearHdr))
        {
            return Err(CaptureRequestError::UnsupportedRecipe);
        }
        if recipe.outputs.contains(&CaptureOutput::LinearHdr)
            && matches!(self.options.frame_profile, crate::FrameProfile::Legacy)
        {
            return Err(CaptureRequestError::HdrRequired);
        }
        let id = self.request_capture_with_dependency(generation, recipe.require_pages)?;
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
