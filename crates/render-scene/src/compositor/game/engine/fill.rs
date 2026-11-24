//! When the engine fills a retained resource of its own, such as the BRDF table or a terrain
//! material array.
//!
//! The fill is staged on the frame that first asks for the resource and encoded in that frame's
//! encoder before its passes draw. Nothing else records that it happened: the frame's own status
//! does. A frame that has submitted has run the fill on the queue, so the resource can be read by
//! every later frame, and a frame that was cancelled or dropped has run nothing.
//!
//! A write set that belongs to a capture admits only the CPU publications its contract declares
//! before the frame is prepared. A resource the engine makes when a game first asks for it has no
//! key to declare, so the fill publishes nothing, and the capture has nothing to refuse.
use worldrunner_render_gpu::submission::{Publications, TransactionStatus};

/// The frame that staged the fill of one resource.
#[derive(Default)]
pub(super) struct Fill {
    staged: Option<Publications>,
}

impl Fill {
    /// Whether `frame` has to stage the fill: not when a submitted frame already ran it, and not
    /// when `frame`, or the capture it is a segment of, has staged it and has not submitted yet.
    pub(super) fn is_needed_in(&self, frame: &Publications) -> bool {
        self.staged
            .as_ref()
            .is_none_or(|staged| match staged.status() {
                TransactionStatus::Submitted => false,
                TransactionStatus::Preparing => !staged.same_frame(frame),
                TransactionStatus::Cancelled => true,
            })
    }

    /// Records that `frame` staged the fill.
    pub(super) fn stage_in(&mut self, frame: Publications) {
        self.staged = Some(frame);
    }
}
