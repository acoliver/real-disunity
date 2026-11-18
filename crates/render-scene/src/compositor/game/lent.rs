//! A value one frame lends to the game's features, readable only while that frame is encoded.
use std::cell::RefCell;
use std::rc::Rc;
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::FrameError;

/// Why a frame cannot lend one of its stock inputs (shadow cascades, ambient occlusion).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameInputError {
    /// No frame is being prepared, so no input is current. An earlier frame's value is never
    /// returned.
    #[error("frame inputs are lent only while a frame is being encoded")]
    OutsideFrame,
    /// The frame in flight published nothing for this input. For the shadow cascades: its route
    /// draws without shadows, shadows are off, or no cascade has a receiver to shade, in which
    /// case the casters wrote nothing this frame.
    #[error("the frame in flight published no value for this input")]
    NotPrepared,
}

impl From<FrameInputError> for FrameError {
    fn from(error: FrameInputError) -> Self {
        Self::FeaturePreparation {
            feature: "frame input",
            reason: error.to_string(),
        }
    }
}

/// The value the frame in flight published, with the frame it belongs to.
#[derive(Debug)]
pub(super) struct Lent<T>(Rc<RefCell<Option<(Publications, T)>>>);

impl<T> Clone for Lent<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> Default for Lent<T> {
    fn default() -> Self {
        Self(Rc::default())
    }
}

impl<T: Clone> Lent<T> {
    /// The value of the frame now being prepared.
    pub(super) fn current(&self) -> Result<T, FrameInputError> {
        let frame = Publications::current().ok_or(FrameInputError::OutsideFrame)?;
        let mut slot = self.0.borrow_mut();
        if let Some((published, value)) = slot.as_ref() {
            if published.same_frame(&frame) {
                return Ok(value.clone());
            }
            // An earlier frame's value (its views, for a texture) is of no use to anyone; do not
            // keep it alive.
            *slot = None;
        }
        Err(FrameInputError::NotPrepared)
    }

    /// Stores `value` as the publication of the frame being prepared. A frame with no
    /// transaction in scope publishes nothing.
    pub(super) fn publish(&self, value: T) {
        if let Some(frame) = Publications::current() {
            *self.0.borrow_mut() = Some((frame, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FrameEnvironmentHandle, FrameOcclusionHandle, FrameShadowHandle};
    use super::FrameInputError;

    /// Outside a frame no input exists; a handle never answers with an earlier frame's value.
    #[test]
    fn outside_a_frame_the_lighting_inputs_are_refused() {
        assert_eq!(
            FrameShadowHandle::default().current().unwrap_err(),
            FrameInputError::OutsideFrame
        );
        assert_eq!(
            FrameOcclusionHandle::default().current().unwrap_err(),
            FrameInputError::OutsideFrame
        );
        assert_eq!(
            FrameEnvironmentHandle::default().current().unwrap_err(),
            FrameInputError::OutsideFrame
        );
    }
}
