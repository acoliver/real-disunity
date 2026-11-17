//! The per-frame path of [`PlayerGpu`]: building a frame's offscreen operations and either
//! running them on the player's own submission or handing them to the viewer.

use std::time::Instant;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_program::unity::offscreen::OffscreenOperations;

use super::{GpuWorkReport, PlayerGpu};
use crate::PlayError;

/// How a frame's operations leave the player.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Delivery {
    /// The player records, submits and waits for them on its own.
    Submit,
    /// The viewer runs them in its next frame.
    Viewer,
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
    /// A frame's operations count as submitted at the start of the next call, so the screen they
    /// drew is read then, one frame after the work, and its image is numbered with the frame that
    /// drew it. The viewer must have run or replaced the operations by then.
    ///
    /// # Errors
    /// As [`Self::frame`].
    pub(crate) fn frame_for_viewer(
        &self,
        host: &mut DisunityHost,
    ) -> Result<Option<OffscreenOperations>, PlayError> {
        self.frame_with(host, Delivery::Viewer)
    }

    fn frame_with(
        &self,
        host: &mut DisunityHost,
        delivery: Delivery,
    ) -> Result<Option<OffscreenOperations>, PlayError> {
        let snapshot = host.graphics_snapshot()?;
        let drawn = self.begin(delivery);
        if snapshot.work.is_empty() {
            let mut report = self.report.borrow_mut();
            // A flush earlier in the update may have drawn into the screen.
            let frame = drawn.unwrap_or(report.frames);
            self.read_screen(&mut report, frame);
            report.frames += 1;
            return Ok(None);
        }
        let started = Instant::now();
        let mut report = self.report.borrow_mut();
        report.frames_with_work += 1;
        let built = self.offscreen.borrow_mut().frame(&snapshot, &*host);
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
        let frame = drawn.unwrap_or(report.frames);
        self.read_screen(&mut report, frame);
        report.frame_time += started.elapsed();
        report.frames += 1;
        Ok(handed)
    }

    /// What a frame does before it builds. For the viewer it takes the operations handed last
    /// time as submitted and frees what the GPU has finished with. Returns the frame whose work
    /// drew into the screen by that submission, if it did.
    fn begin(&self, delivery: Delivery) -> Option<u64> {
        if delivery == Delivery::Submit {
            return None;
        }
        let mut host = self.offscreen.borrow_mut();
        let flushed = host.screen_written();
        host.frame_submitted();
        host.collect_completed();
        self.handed
            .take()
            .filter(|_| host.screen_written() && !flushed)
    }

    /// Runs `operations` as `delivery` says. The viewer's are given back to its caller.
    fn deliver(
        &self,
        report: &mut GpuWorkReport,
        operations: OffscreenOperations,
        delivery: Delivery,
    ) -> Option<OffscreenOperations> {
        if delivery == Delivery::Viewer {
            self.handed.set(Some(report.frames));
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
