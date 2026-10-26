use super::{RetainedEvent, RetainedHost};
use std::collections::{BTreeMap, VecDeque};
use worldrunner_render_scene::offscreen::RgbaImage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedCaptureOutcome {
    Completed {
        frame: u64,
        time: std::time::Duration,
        image: RgbaImage,
    },
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CaptureRequestError {
    #[error("capture admission closed or bound {limit} reached")]
    BackPressure { limit: usize },
    #[error("generation {generation} has not been accepted")]
    UnknownGeneration { generation: u64 },
    #[error("generation {generation} was superseded by {ready}")]
    Superseded { generation: u64, ready: u64 },
    #[error("host is exiting")]
    Exiting,
}

pub(crate) struct Captures {
    pub waiting: VecDeque<(u64, u64)>,
    outstanding: BTreeMap<u64, u64>,
    pub events: VecDeque<RetainedEvent>,
    limit: usize,
    next: u64,
}
impl Default for Captures {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
            outstanding: BTreeMap::new(),
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
    pub(crate) fn take_event(&mut self) -> Option<RetainedEvent> {
        let event = self.events.pop_front()?;
        if let RetainedEvent::Capture { id, .. } = &event {
            self.outstanding.remove(id).expect("outstanding capture");
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
        if self.exit {
            return Err(CaptureRequestError::Exiting);
        }
        if generation > self.accepted_generation() {
            return Err(CaptureRequestError::UnknownGeneration { generation });
        }
        if generation < self.ready_generation() {
            return Err(CaptureRequestError::Superseded {
                generation,
                ready: self.ready_generation(),
            });
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
        self.captures.waiting.push_back((id, generation));
        Ok(id)
    }
}

#[cfg(test)]
mod tests;
