#[path = "diagnostics.rs"]
pub(crate) mod diagnostics;
#[path = "desktop_strict.rs"]
mod strict_desktop;

use crate::{
    interactive::{Config, Continuous},
    live_input::{Command, Input},
    playback::Playback,
};
use serde_json::{Value, json};
use worldrunner::scene::viewer::{LiveDriver, LiveFrame, LiveOptions, LiveReceipt, LiveTick};

pub(crate) fn run(playback: Playback<'_>) -> Result<(), String> {
    let mut driver = Desktop::new(playback)?;
    let mut options = LiveOptions::new(
        format!(
            "Real Disunity: {} [{}]",
            driver.playback.request.fixture, driver.playback.request.profile
        ),
        [
            driver.playback.viewport.width,
            driver.playback.viewport.height,
        ],
    );
    options.max_frames = Some(driver.frame_limit()?);
    let profile = diagnostics::begin(driver.strict.is_some());
    let outcome = worldrunner::scene::viewer::run_live(&mut driver, options)
        .map_err(|error| error.to_string());
    if let Err(error) = diagnostics::flush(&driver.playback.request.artifact_root, profile) {
        driver.diagnostic_failure(format!("presentation profile write failed: {error}"));
    }
    outcome
}

struct Desktop<'a> {
    playback: Playback<'a>,
    cursor: usize,
    input: Input,
    uploaded: bool,
    continuous: Option<Continuous>,
    strict: Option<crate::strict_ingress::Staged>,
    prewarmed: bool,
}

impl<'a> Desktop<'a> {
    fn new(playback: Playback<'a>) -> Result<Self, String> {
        let continuous = Config::admit(playback.request)?.map(Continuous::new);
        let strict = crate::strict::selected(&playback.request.metadata)?
            .then(crate::strict_ingress::Staged::default);
        let prewarmed = crate::strict::startup(
            &playback.request.metadata,
            strict.is_some() && continuous.is_some(),
        )?;
        Ok(Self {
            playback,
            cursor: 0,
            input: Input::default(),
            uploaded: false,
            continuous,
            strict,
            prewarmed,
        })
    }

    fn frame_limit(&self) -> Result<u64, String> {
        if let Some(continuous) = &self.continuous {
            return Ok(continuous.config.max_frames);
        }
        self.playback
            .request
            .timeline
            .iter()
            .filter_map(|action| action["frame"].as_u64())
            .max()
            .ok_or("desktop timeline is empty")?
            .checked_add(1)
            .ok_or("desktop frame limit overflow".into())
    }

    fn apply_input(&mut self) -> Result<(), String> {
        let (keys, axes) = self.input.state();
        self.playback.active()?.input(keys, axes)
    }

    fn action(&mut self, action: &Value) -> Result<(), String> {
        crate::strict::action_mode(
            self.strict.is_some(),
            action["action"].as_str().unwrap_or(""),
        )?;
        if action["action"] == "input" {
            self.input.script(action)?;
            self.apply_input()
        } else {
            self.playback.action(action)
        }
    }

    fn timeline(&mut self, frame: u64) -> Result<(), String> {
        while let Some(action) = self.playback.request.timeline.get(self.cursor) {
            let at = action["frame"]
                .as_u64()
                .ok_or("timeline entry has no frame")?;
            if at > frame {
                break;
            }
            if at < frame {
                return Err("desktop timeline is not in nondecreasing frame order".into());
            }
            let action = action.clone();
            self.action(&action)?;
            self.cursor += 1;
        }
        Ok(())
    }

    fn continuous(&mut self, tick: &LiveTick, commands: &[Command]) -> Result<bool, String> {
        let stop = commands.contains(&Command::Stop);
        let reload = commands.contains(&Command::Reload) && !stop;
        if reload {
            self.input.clear();
            self.playback
                .action(&json!({"frame":tick.frame,"phase":"reload","action":"reload"}))?;
            self.apply_input()?;
        }
        let steps = if stop || reload {
            Vec::new()
        } else {
            self.continuous
                .as_mut()
                .ok_or("interactive state missing")?
                .steps(tick)
        };
        for dt in &steps {
            self.playback.active()?.advance(*dt)?;
        }
        let state = self
            .continuous
            .as_ref()
            .ok_or("interactive state missing")?;
        let finished = stop || state.finished(tick);
        let journal = json!({"stage":"update","frame":tick.frame,"elapsed":tick.elapsed.as_secs_f64(),
            "dt":tick.dt.as_secs_f64(),"steps":steps,"simulated":state.simulated(),
            "keys":self.input.state().0,"reload":reload,"stop":finished,
            "input":format!("{:?}",tick.input)});
        crate::interactive_evidence::journal(&self.playback.request.artifact_root, &journal)?;
        if tick.frame == 0 || finished || commands.contains(&Command::Capture) {
            self.capture(tick.frame)?;
        }
        Ok(finished)
    }

    fn capture(&mut self, frame: u64) -> Result<(), String> {
        let state = self
            .continuous
            .as_mut()
            .ok_or("interactive state missing")?;
        if state.captures >= state.config.max_captures {
            return Ok(());
        }
        state.captures += 1;
        crate::interactive_evidence::capture(&mut self.playback, frame)
    }

    fn frame(&mut self, tick: &LiveTick, exit: bool) -> Result<LiveFrame, String> {
        let contribution = self.playback.contribution()?;
        let changed = self.playback.textures_changed();
        let textures = if self.uploaded && !changed {
            None
        } else {
            self.uploaded = true;
            Some(self.playback.textures()?)
        };
        Ok(LiveFrame {
            view: contribution.view,
            quads: contribution.quads,
            textures,
            clear: contribution.clear,
            capture: self.playback.pending_capture.as_ref().map(|_| tick.frame),
            exit,
        })
    }
}

impl LiveDriver for Desktop<'_> {
    fn clock(&self) -> worldrunner::scene::viewer::LiveClock {
        if self.prewarmed {
            worldrunner::scene::viewer::LiveClock::ReadyPrewarmed
        } else if self.strict.is_some() {
            worldrunner::scene::viewer::LiveClock::Acquisition
        } else {
            worldrunner::scene::viewer::LiveClock::Legacy
        }
    }

    fn initialize(&mut self, viewport: worldrunner::scene::gpu::Viewport) -> Result<(), String> {
        if self.continuous.is_some() {
            self.playback.viewport = viewport;
            self.timeline(0)?;
        }
        Ok(())
    }

    fn prepare_initial(
        &mut self,
        viewport: worldrunner::scene::gpu::Viewport,
    ) -> Result<Option<LiveFrame>, String> {
        if !self.prewarmed {
            return Ok(None);
        }
        if self.playback.pending_capture.is_some() || self.playback.pending_unload {
            return Err("initial scene cannot carry a pending capture or unload".into());
        }
        self.playback.viewport = viewport;
        let contribution = self.playback.contribution()?;
        self.uploaded = true;
        let frame = LiveFrame {
            view: contribution.view,
            quads: contribution.quads,
            textures: Some(self.playback.textures()?),
            clear: contribution.clear,
            capture: None,
            exit: false,
        };
        crate::interactive_evidence::journal(
            &self.playback.request.artifact_root,
            &json!({"stage":"startup_prepared","startup":crate::strict::STARTUP,
                "quads":frame.quads.len(),"viewport":[viewport.width,viewport.height]}),
        )?;
        Ok(Some(frame))
    }

    fn ready(&mut self) -> Result<(), String> {
        crate::interactive_evidence::journal(
            &self.playback.request.artifact_root,
            &json!({"stage":"ready","startup":crate::strict::STARTUP,
                "execution_profile":crate::strict::PROFILE}),
        )
    }

    fn ingress(
        &mut self,
        sample: &worldrunner::scene::viewer::LiveInputSample,
    ) -> Result<(), String> {
        if let Some(staged) = &mut self.strict {
            staged.stage(sample)?;
        }
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        if self.strict.is_some() {
            return self.strict_update(tick);
        }
        self.playback.viewport = tick.viewport;
        let interactive = self.continuous.is_some();
        let ingress = self.input.ingress(&tick.input, interactive);
        if ingress.changed && self.playback.session.is_some() {
            self.apply_input()?;
        }
        self.timeline(tick.frame)?;
        let exit = if interactive {
            if tick.frame == 0 {
                self.apply_input()?;
            }
            self.continuous(tick, &ingress.commands)?
        } else {
            ingress.commands.contains(&Command::Stop)
                || self.playback.pending_unload
                || self.cursor == self.playback.request.timeline.len()
        };
        self.frame(tick, exit)
    }

    fn presented(&mut self, receipt: LiveReceipt) -> Result<(), String> {
        let _span = worldrunner::scene::viewer::cpu_profile::CpuProfile::span("driver_presented");
        if let Some(capture) = receipt.capture {
            let pending = self
                .playback
                .pending_capture
                .take()
                .ok_or("unexpected desktop capture receipt")?;
            if capture.id != receipt.frame || pending.frame != receipt.frame {
                return Err("desktop capture frame correlation failed".into());
            }
            self.playback.save(pending, &capture.image)?;
        } else if self.playback.pending_capture.is_some() {
            return Err("desktop presentation omitted requested capture".into());
        }
        if self.continuous.is_some() {
            crate::interactive_evidence::journal(
                &self.playback.request.artifact_root,
                &json!({"stage":"presented","frame":receipt.frame,"elapsed":receipt.elapsed.as_secs_f64()}),
            )?;
        }
        if self.playback.pending_unload {
            self.playback.active()?.unload()?;
            self.playback.pending_unload = false;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "desktop_strict_tests.rs"]
mod strict_tests;

#[cfg(test)]
#[path = "desktop_startup_tests.rs"]
mod startup_tests;
