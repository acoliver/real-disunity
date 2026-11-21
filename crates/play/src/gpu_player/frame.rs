//! The per-frame path of [`PlayerGpu`]: building a frame's offscreen operations and either
//! running them on the player's own submission or handing them to the viewer.

use std::time::Instant;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_program::offscreen::PendingOffscreen;
use worldrunner_render_program::unity::offscreen::OffscreenOperations;

use super::{GpuWorkReport, PlayerGpu};
use crate::PlayError;
use crate::offscreen::Settling;

/// How a frame's operations leave the player.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Delivery {
    /// The player records, submits and waits for them on its own.
    Submit,
    /// The viewer runs them in its next frame.
    Viewer,
}

impl Delivery {
    /// How the frame's reclaiming gets the GPU to finish with the storage it gives back. The
    /// viewer's submissions are not the player's to wait for.
    fn settling(self) -> Settling {
        match self {
            Self::Submit => Settling::Wait,
            Self::Viewer => Settling::Poll,
        }
    }
}

impl PlayerGpu {
    /// Takes the frame's snapshot and runs its offscreen work: records it, submits it and waits
    /// for it on the player's own. A frame with none returns at once and leaves the device alone.
    ///
    /// # Errors
    /// Fails when the host cannot give the snapshot. Work the GPU refuses is counted in the
    /// report, not an error: the player goes on without it.
    pub fn frame(&self, host: &mut DisunityHost) -> Result<(), PlayError> {
        self.frame_with(host, Delivery::Submit).map(drop)
    }

    /// Takes the frame's snapshot and builds its offscreen work for the viewer, which admits it
    /// on its own recording and runs it in the one submission of its next frame. Nothing is
    /// submitted and nothing waits on the GPU here: storage the GPU has finished with is freed,
    /// and the rest goes when a later frame finds its submission complete.
    ///
    /// Operations that draw into the screen name it as the viewer's game screen, so the frame that
    /// runs them shows the result; the texture is [`Self::game_screen`]. Nothing is read back for
    /// the window. Operations the viewer replaces before it renders never run, and the viewer
    /// counts only a screen it submitted a write to.
    ///
    /// # Errors
    /// As [`Self::frame`].
    pub(crate) fn frame_for_viewer(
        &self,
        host: &mut DisunityHost,
    ) -> Result<Option<PendingOffscreen>, PlayError> {
        self.frame_with(host, Delivery::Viewer)
            .map(|operations| operations.map(OffscreenOperations::into_pending))
    }

    fn frame_with(
        &self,
        host: &mut DisunityHost,
        delivery: Delivery,
    ) -> Result<Option<OffscreenOperations>, PlayError> {
        let snapshot = host.graphics_snapshot()?;
        self.begin(delivery);
        if snapshot.work.is_empty() {
            let mut report = self.report.borrow_mut();
            // A flush earlier in the update may have drawn into the screen.
            self.note_for_player(delivery, report.frames);
            report.frames += 1;
            return Ok(None);
        }
        let started = Instant::now();
        let mut report = self.report.borrow_mut();
        report.frames_with_work += 1;
        let built = self
            .offscreen
            .borrow_mut()
            .frame_with(&snapshot, &*host, delivery.settling());
        report.keep_traces(self.offscreen.borrow_mut().take_traces());
        let mut handed = None;
        match built {
            Ok((operations, frame_report)) => {
                report.add(&frame_report, "frame");
                if let Some(operations) = operations {
                    handed = self.deliver(&mut report, operations, delivery);
                }
            }
            Err(error) => {
                *report.errors.entry(error.to_string()).or_default() += 1;
            }
        }
        report.peak_runtime_bytes = self.offscreen.borrow().peak_bytes();
        self.note_for_player(delivery, report.frames);
        report.frame_time += started.elapsed();
        report.frames += 1;
        Ok(handed)
    }

    /// What a frame does before it builds. For the viewer it frees the storage the GPU has
    /// finished with, without waiting for anything.
    fn begin(&self, delivery: Delivery) {
        if delivery == Delivery::Viewer {
            self.offscreen.borrow_mut().collect_completed();
        }
    }

    /// Numbers the screen for the player's own frames, which have the image written to a file
    /// from the screen texture. The viewer shows the screen from the GPU, so its frames note
    /// nothing.
    fn note_for_player(&self, delivery: Delivery, frame: u64) {
        if delivery == Delivery::Submit {
            self.note_drawn_screen(frame);
        }
    }

    /// Runs `operations` as `delivery` says. The viewer's are given back to its caller, with the
    /// screen exported from them when they write it. An export the operations refuse is counted
    /// and the operations still go: the viewer then has no write to count for the screen.
    fn deliver(
        &self,
        report: &mut GpuWorkReport,
        mut operations: OffscreenOperations,
        delivery: Delivery,
    ) -> Option<OffscreenOperations> {
        if delivery == Delivery::Viewer {
            if let Err(error) = self.offscreen.borrow().export_screen(&mut operations) {
                *report.errors.entry(error.to_string()).or_default() += 1;
            }
            return Some(operations);
        }
        let submitted = Instant::now();
        let ran = self.run(operations);
        self.offscreen.borrow_mut().settle();
        if let Some(work) = report.work.last_mut() {
            work.phases.submit += submitted.elapsed();
        }
        if let Err(message) = ran {
            *report.errors.entry(message).or_default() += 1;
        }
        None
    }

    /// Records the operations into a frame of their own, submits it and waits for it.
    fn run(&self, operations: OffscreenOperations) -> Result<(), String> {
        let mut frame = self
            .authority
            .begin(&self.device)
            .map_err(|error| error.to_string())?;
        let admitted = operations
            .admit(&frame)
            .map_err(|error| error.to_string())?;
        admitted
            .encode(None, &mut frame)
            .map_err(|error| error.to_string())?;
        let submission = frame.finish().submit().map_err(|error| error.to_string())?;
        self.offscreen.borrow_mut().frame_submitted();
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}
