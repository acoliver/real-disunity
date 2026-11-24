//! The per-frame path of [`PlayerGpu`]: building a frame's offscreen operations and either
//! running them on the player's own submission or handing them to the viewer.

use std::time::Instant;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_program::offscreen::PendingOffscreen;
use worldrunner_render_program::unity::offscreen::OffscreenOperations;

use super::{GpuWorkReport, PlayerGpu};
use crate::offscreen::{Assets, Settling, UiFrame};
use crate::target_read::{Finished, finish, refuse};
use crate::{PlayError, TargetRead};

/// The reads a frame was asked for, and what the build called each record.
type Reads<'a> = (&'a [TargetRead], &'a [(usize, String)]);

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
    /// report, not an error here: the player goes on without it, and a headless run ends at it
    /// through `take_refusal`.
    pub fn frame(&self, host: &mut DisunityHost) -> Result<(), PlayError> {
        self.frame_with(host, Delivery::Submit, None).map(drop)
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
    /// `ui` is the work that draws the runs of UI draws a game's shader draws. It runs after the
    /// frame's own work, into the layer atlas ([`Self::ui_layers`]), in the same operations.
    ///
    /// # Errors
    /// As [`Self::frame`].
    pub(crate) fn frame_for_viewer(
        &self,
        host: &mut DisunityHost,
        ui: Option<&UiFrame>,
    ) -> Result<Option<PendingOffscreen>, PlayError> {
        self.frame_with(host, Delivery::Viewer, ui)
            .map(|operations| operations.map(OffscreenOperations::into_pending))
    }

    fn frame_with(
        &self,
        host: &mut DisunityHost,
        delivery: Delivery,
        ui: Option<&UiFrame>,
    ) -> Result<Option<OffscreenOperations>, PlayError> {
        let mut snapshot = host.graphics_snapshot()?;
        if let Some(ui) = ui {
            ui.join(&mut snapshot);
        }
        self.ui_atlas.set(ui.map(UiFrame::atlas_size));
        self.begin(delivery);
        let reads = self.reads_due(delivery);
        if snapshot.work.is_empty() {
            let mut report = self.report.borrow_mut();
            refuse(&reads, "the frame has no offscreen work").add_to(&mut report);
            // A flush earlier in the update may have drawn into the screen.
            self.note_drawn_screen(report.frames);
            report.frames += 1;
            return Ok(None);
        }
        let started = Instant::now();
        let mut report = self.report.borrow_mut();
        report.frames_with_work += 1;
        let host_assets: &dyn Assets = &*host;
        let ui_assets = ui.map(|ui| ui.assets(host_assets));
        let assets: &dyn Assets = ui_assets.as_ref().map_or(host_assets, |assets| assets);
        let built =
            self.offscreen
                .borrow_mut()
                .frame(&snapshot, assets, delivery.settling(), &reads);
        report.keep_traces(self.offscreen.borrow_mut().take_traces());
        let mut handed = None;
        match built {
            Ok((operations, frame_report)) => {
                report.add(&frame_report, "frame");
                match operations {
                    Some(operations) => {
                        let labels = frame_report.reads;
                        handed = self.deliver(&mut report, operations, delivery, (&reads, &labels));
                    }
                    None => refuse(&reads, "the frame has no records").add_to(&mut report),
                }
            }
            Err(error) => {
                refuse(&reads, &format!("the frame did not build: {error}")).add_to(&mut report);
                *report.errors.entry(error.to_string()).or_default() += 1;
            }
        }
        report.peak_runtime_bytes = self.offscreen.borrow().peak_bytes();
        self.note_drawn_screen(report.frames);
        report.frame_time += started.elapsed();
        report.frames += 1;
        Ok(handed)
    }

    /// The reads the run asked for of the frame that is about to be built. A window's frames are
    /// submitted by the viewer, which has no submission to wait for, so they are refused.
    fn reads_due(&self, delivery: Delivery) -> Vec<TargetRead> {
        let frame = self.report.borrow().frames;
        let due: Vec<TargetRead> = self
            .reads
            .borrow()
            .iter()
            .filter(|read| read.frame == frame)
            .cloned()
            .collect();
        if delivery == Delivery::Viewer && !due.is_empty() {
            let why = "a window's frames are submitted by the viewer";
            refuse(&due, why).add_to(&mut self.report.borrow_mut());
            return Vec::new();
        }
        due
    }

    /// What a frame does before it builds. For the viewer it frees the storage the GPU has
    /// finished with, without waiting for anything.
    fn begin(&self, delivery: Delivery) {
        if delivery == Delivery::Viewer {
            self.offscreen.borrow_mut().collect_completed();
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
        reads: Reads<'_>,
    ) -> Option<OffscreenOperations> {
        if delivery == Delivery::Viewer {
            if let Err(error) = self.offscreen.borrow().export_screen(&mut operations) {
                *report.errors.entry(error.to_string()).or_default() += 1;
            }
            // The viewer runs them in the frame this update's capture waits for.
            self.offscreen.borrow_mut().frame_submitted();
            return Some(operations);
        }
        let submitted = Instant::now();
        let ran = self.run(operations, reads);
        self.offscreen.borrow_mut().settle();
        if let Some(work) = report.work.last_mut() {
            work.phases.submit += submitted.elapsed();
        }
        match ran {
            Ok(finished) => finished.add_to(report),
            Err(message) => {
                refuse(reads.0, &format!("the frame did not run: {message}")).add_to(report);
                *report.errors.entry(message).or_default() += 1;
            }
        }
        None
    }

    /// Records the operations into a frame of their own, submits it and waits for it, then writes
    /// the files of the frame's reads.
    fn run(
        &self,
        mut operations: OffscreenOperations,
        (reads, labels): Reads<'_>,
    ) -> Result<Finished, String> {
        let taps = operations.take_taps();
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
        let missing =
            "the build left out records of the frame and built it again without the reads";
        Ok(finish(reads, labels, taps, &self.device, missing))
    }
}
