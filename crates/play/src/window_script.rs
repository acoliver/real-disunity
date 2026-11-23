//! What a command line scripts in a play window: the keys that arrive at chosen game frames, the
//! step each frame takes, the frames kept as captures and the frame after which the window closes.
//! The original games and the built players run the same script, so the two drivers differ only in
//! what they extract from the world.
//!
//! Frames are the game's: frame N is the Nth time the script advanced the game, counting from zero.
//! The viewer may update more often than that. It repeats an update until a frame is submitted, and
//! a requested capture is taken by a later frame than the one that asked for it. While a capture
//! is outstanding the script holds the game where it is, so the image is the one after the frame
//! that asked for it, with that frame's UI.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_scene_viewer::{
    RetainedCaptureOutcome, RetainedEvent, RetainedEventHost, RetainedHost, RetainedInput,
    RetainedTick,
};

use crate::{PlayError, PlayerWindow, ScheduledKey, driver::Capture, keys::key_code};

/// What one update of the viewer does with the game.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Turn {
    /// The game stays as it is: a requested capture has not arrived, or the last frame is done.
    Held,
    /// The game receives `keys` and advances by `step`, which makes it frame `frame`.
    Advance {
        frame: u64,
        keys: Vec<(u16, bool)>,
        step: Duration,
    },
}

pub(crate) struct WindowScript {
    keys: Vec<ScheduledKey>,
    fixed_dt: Option<Duration>,
    max_frames: Option<u64>,
    capture_frames: BTreeSet<u64>,
    /// The files of the captures that are not written yet, by game frame.
    unwritten: Vec<(u64, PathBuf)>,
    /// The game time the frames so far advanced.
    simulated: Duration,
    /// The game frames advanced so far.
    advanced: u64,
    /// Requested captures not delivered yet, by request id, with the game frame they belong to.
    pending: BTreeMap<u64, u64>,
    captures: Vec<Capture>,
    /// When the oldest outstanding capture was asked for.
    asked: Option<Instant>,
    /// When the game frame whose time is running began.
    started: Option<Instant>,
    /// How long each game frame took from its update to its presented frame, or to its capture
    /// when it asked for one.
    times: Vec<Duration>,
}

impl WindowScript {
    pub(crate) fn new(window: &PlayerWindow) -> Self {
        Self {
            keys: window.keys.clone(),
            fixed_dt: window.fixed_dt,
            max_frames: window.max_frames,
            capture_frames: window.captured_frames(),
            unwritten: window.capture_files.clone(),
            simulated: Duration::ZERO,
            advanced: 0,
            pending: BTreeMap::new(),
            captures: Vec::new(),
            asked: None,
            started: None,
            times: Vec::new(),
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

    /// What this update does. It holds the game while a capture is outstanding and once the last
    /// frame is done. Otherwise it is the next game frame, with the keys scheduled for that frame
    /// and the frame's step: the fixed one when the window has it, else the time the viewer took
    /// (none for the first frame).
    pub(crate) fn next(&mut self, tick: &RetainedTick) -> Turn {
        let done = self.max_frames.is_some_and(|max| self.advanced >= max);
        if done || !self.pending.is_empty() {
            return Turn::Held;
        }
        let frame = self.advanced;
        self.advanced += 1;
        Turn::Advance {
            frame,
            keys: self
                .keys
                .iter()
                .filter(|key| key.frame == frame)
                .map(|key| (key.code, key.pressed))
                .collect(),
            step: self
                .fixed_dt
                .unwrap_or(if frame == 0 { Duration::ZERO } else { tick.dt }),
        }
    }

    /// Delivers the keys of an advancing `turn` to the game, then advances it by the turn's step.
    pub(crate) fn advance(
        &mut self,
        host: &mut DisunityHost,
        turn: &Turn,
    ) -> Result<(), PlayError> {
        let Turn::Advance { keys, step, .. } = turn else {
            return Ok(());
        };
        self.started = Some(Instant::now());
        for (code, pressed) in keys {
            self.push_key(host, *code, *pressed)?;
        }
        let nanos = u64::try_from(step.as_nanos())
            .map_err(|_| PlayError::Invalid("frame delta too long".into()))?;
        host.advance(nanos, 1_000_000_000)?;
        self.simulated += *step;
        Ok(())
    }

    /// The game frames advanced so far.
    pub(crate) fn advanced(&self) -> u64 {
        self.advanced
    }

    /// How long each game frame took, as the window and the headless runs report it.
    pub(crate) fn times(&self) -> &[Duration] {
        &self.times
    }

    /// The game frame the last update advanced to, when the window keeps it and has not asked for
    /// it yet.
    pub(crate) fn due_capture(&self) -> Option<u64> {
        let frame = self.advanced.checked_sub(1)?;
        (self.capture_frames.contains(&frame) && !self.pending.values().any(|held| *held == frame))
            .then_some(frame)
    }

    /// Notes that the capture of `frame` was asked for as `id`.
    pub(crate) fn requested(&mut self, id: u64, frame: u64) {
        self.pending.insert(id, frame);
        self.asked.get_or_insert_with(Instant::now);
    }

    /// The oldest outstanding capture's frame, when it was asked for longer than `limit` ago.
    pub(crate) fn overdue(&self, limit: Duration) -> Option<u64> {
        if self.asked?.elapsed() <= limit {
            return None;
        }
        self.pending.values().min().copied()
    }

    /// Asks for the frame the last update advanced to, when the window keeps it.
    pub(crate) fn request_capture(&mut self, viewer: &mut RetainedHost) -> Result<(), PlayError> {
        let Some(frame) = self.due_capture() else {
            return Ok(());
        };
        let id = viewer
            .request_capture(viewer.accepted_generation())
            .map_err(|error| PlayError::Invalid(format!("capture {frame}: {error}")))?;
        self.requested(id, frame);
        Ok(())
    }

    /// Handles what the window reports: a key the game has reaches it, presented frames close the
    /// time of the game frame that made them, delivered captures are kept, and the window closes
    /// once the script has run.
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
            RetainedEvent::Presented { .. } => self.close_time(),
            RetainedEvent::Capture { id, outcome, .. } => {
                self.delivered(*id, outcome)
                    .map_err(|error| error.to_string())?;
                self.close_time();
            }
            _ => {}
        }
        if self.finished() {
            viewer.request_exit();
        }
        Ok(())
    }

    /// Ends the running game frame's time unless it waits for its capture.
    fn close_time(&mut self) {
        if !self.pending.is_empty() {
            return;
        }
        if let Some(started) = self.started.take() {
            self.times.push(started.elapsed());
        }
    }

    /// Keeps the capture a request got and writes it to every file asked for its frame now, so a
    /// run that stops later has the frames it reached.
    pub(crate) fn delivered(
        &mut self,
        id: u64,
        outcome: &RetainedCaptureOutcome,
    ) -> Result<(), PlayError> {
        let Some(frame) = self.pending.remove(&id) else {
            return Ok(());
        };
        if self.pending.is_empty() {
            self.asked = None;
        }
        if let RetainedCaptureOutcome::Completed { image, .. } = outcome {
            let capture = Capture {
                frame,
                width: image.viewport.width,
                height: image.viewport.height,
                pixels: image.pixels.clone(),
            };
            self.write_files(&capture)?;
            self.captures.push(capture);
        }
        Ok(())
    }

    fn write_files(&mut self, capture: &Capture) -> Result<(), PlayError> {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.unwritten)
            .into_iter()
            .partition(|(frame, _)| *frame == capture.frame);
        self.unwritten = later;
        for (frame, file) in due {
            capture.write_png(&file)?;
            println!("frame {frame} captured to {}", file.display());
        }
        Ok(())
    }

    /// Whether the last game frame is advanced and every capture arrived.
    pub(crate) fn finished(&self) -> bool {
        self.max_frames.is_some_and(|max| self.advanced >= max) && self.pending.is_empty()
    }

    /// The captures the run kept, once every file asked for has been written.
    ///
    /// # Errors
    /// Fails naming each frame and file that no capture reached.
    pub(crate) fn finish(&mut self) -> Result<Vec<Capture>, PlayError> {
        if !self.unwritten.is_empty() {
            let missing: Vec<String> = self
                .unwritten
                .iter()
                .map(|(frame, file)| format!("frame {frame} to {}", file.display()))
                .collect();
            return Err(PlayError::Invalid(format!(
                "{} requested frames were not captured: {}",
                missing.len(),
                missing.join(", ")
            )));
        }
        Ok(std::mem::take(&mut self.captures))
    }
}

#[cfg(test)]
#[path = "window_script_tests.rs"]
mod tests;
