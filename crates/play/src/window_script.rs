//! What a command line scripts in a play window: the keys that arrive at chosen updates, the step
//! each update takes, the frames kept as captures and the update after which the window closes.
//! The original games and the built players run the same script, so the two drivers differ only in
//! what they extract from the world.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_scene_viewer::{
    RetainedCaptureOutcome, RetainedEvent, RetainedEventHost, RetainedHost, RetainedInput,
    RetainedTick,
};

use crate::{PlayError, PlayerWindow, ScheduledKey, driver::Capture, keys::key_code};

pub(crate) struct WindowScript {
    keys: Vec<ScheduledKey>,
    fixed_dt: Option<Duration>,
    max_frames: Option<u64>,
    capture_frames: BTreeSet<u64>,
    /// The game time the updates so far advanced.
    simulated: Duration,
    presented: u64,
    /// Requested captures not delivered yet, by request id, with the update they belong to.
    pending: BTreeMap<u64, u64>,
    captures: Vec<Capture>,
}

impl WindowScript {
    pub(crate) fn new(window: &PlayerWindow) -> Self {
        Self {
            keys: window.keys.clone(),
            fixed_dt: window.fixed_dt,
            max_frames: window.max_frames,
            capture_frames: window.capture_frames.clone(),
            simulated: Duration::ZERO,
            presented: 0,
            pending: BTreeMap::new(),
            captures: Vec::new(),
        }
    }

    /// Sends a key to the game at the time it arrives: after the last advanced frame, where the
    /// next update reads it.
    pub(crate) fn push_key(
        &self,
        host: &mut DisunityHost,
        code: u16,
        pressed: bool,
    ) -> Result<(), PlayError> {
        let at = self.simulated + Duration::from_nanos(1);
        Ok(host.push_key(at, code, pressed)?)
    }

    /// Delivers the keys scheduled for this update, then advances the game by the update's step:
    /// the fixed one when the window has it, else the time the window took (none for the first
    /// update).
    pub(crate) fn advance(
        &mut self,
        host: &mut DisunityHost,
        tick: &RetainedTick,
    ) -> Result<(), PlayError> {
        for key in self.keys.iter().filter(|key| key.frame == tick.frame) {
            self.push_key(host, key.code, key.pressed)?;
        }
        let dt = self.fixed_dt.unwrap_or(if tick.frame == 0 {
            Duration::ZERO
        } else {
            tick.dt
        });
        let nanos = u64::try_from(dt.as_nanos())
            .map_err(|_| PlayError::Invalid("frame delta too long".into()))?;
        host.advance(nanos, 1_000_000_000)?;
        self.simulated += dt;
        Ok(())
    }

    /// Asks for the frame this update shows when the window keeps it.
    pub(crate) fn request_capture(
        &mut self,
        viewer: &mut RetainedHost,
        frame: u64,
    ) -> Result<(), PlayError> {
        if !self.capture_frames.contains(&frame) {
            return Ok(());
        }
        let id = viewer
            .request_capture(viewer.accepted_generation())
            .map_err(|error| PlayError::Invalid(format!("capture {frame}: {error}")))?;
        self.pending.insert(id, frame);
        Ok(())
    }

    /// Handles what the window reports: a key the game has reaches it, presented updates and
    /// delivered captures are counted, and the window closes once the script has run.
    pub(crate) fn event(
        &mut self,
        host: &mut DisunityHost,
        viewer: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        match event {
            RetainedEvent::Input(RetainedInput::Key {
                key,
                pressed,
                repeat: false,
                ..
            }) => {
                if let Some(code) = key_code(key) {
                    self.push_key(host, code, *pressed)
                        .map_err(|error| error.to_string())?;
                }
            }
            RetainedEvent::Presented { frame } => self.presented = frame + 1,
            RetainedEvent::Capture { id, outcome, .. } => self.delivered(*id, outcome),
            _ => {}
        }
        if self.finished() {
            viewer.request_exit();
        }
        Ok(())
    }

    fn delivered(&mut self, id: u64, outcome: &RetainedCaptureOutcome) {
        let Some(frame) = self.pending.remove(&id) else {
            return;
        };
        if let RetainedCaptureOutcome::Completed { image, .. } = outcome {
            self.captures.push(Capture {
                frame,
                width: image.viewport.width,
                height: image.viewport.height,
                pixels: image.pixels.clone(),
            });
        }
    }

    /// Whether the last update the window was asked for is presented and every capture arrived.
    pub(crate) fn finished(&self) -> bool {
        self.max_frames.is_some_and(|max| self.presented >= max) && self.pending.is_empty()
    }

    pub(crate) fn take_captures(&mut self) -> Vec<Capture> {
        std::mem::take(&mut self.captures)
    }
}
