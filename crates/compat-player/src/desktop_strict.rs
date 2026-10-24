use super::Desktop;
use crate::live_input::Command;
use serde_json::{Value, json};
use std::time::Duration;
use worldrunner::scene::viewer::{LiveFrame, LiveInputEvent, LiveTick};

impl Desktop<'_> {
    pub(super) fn strict_update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        use worldrunner::scene::viewer::cpu_profile::CpuProfile;
        CpuProfile::frame(tick.frame);
        let _span = CpuProfile::span("driver_update");
        let steps = match crate::strict::steps(tick.dt) {
            Ok(steps) => steps,
            Err(error) => return Err(self.reject_elapsed(tick, error)),
        };
        let events = self
            .strict
            .as_ref()
            .ok_or("strict ingress missing")?
            .batch()?;
        let endpoint = tick
            .elapsed
            .checked_add(tick.dt)
            .ok_or("strict elapsed overflow")?;
        let controls: Vec<_> = tick
            .input
            .iter()
            .filter(|event| host_event(event))
            .cloned()
            .collect();
        let ingress = self.input.ingress(&controls, true);
        self.playback.viewport = tick.viewport;
        let cursor = self.cursor;
        self.timeline(tick.frame)?;
        let timeline_reload = self.playback.request.timeline[cursor..self.cursor]
            .iter()
            .any(|action| action["action"] == "reload");
        let reload = timeline_reload
            || (ingress.commands.contains(&Command::Reload)
                && !ingress.commands.contains(&Command::Stop));
        if reload {
            if !timeline_reload {
                self.playback
                    .action(&json!({"frame":tick.frame,"phase":"reload","action":"reload"}))?;
            }
            self.strict
                .as_mut()
                .ok_or("strict ingress missing")?
                .retire(endpoint);
        } else {
            self.strict_submit(&events)?;
        }
        let exit = if self.continuous.is_some() {
            self.strict_continuous(tick, &steps, &ingress.commands, reload)?
        } else {
            ingress.commands.contains(&Command::Stop)
                || self.playback.pending_unload
                || self.cursor == self.playback.request.timeline.len()
        };
        crate::interactive_evidence::journal(
            &self.playback.request.artifact_root,
            &json!({"stage":"strict_ingress","frame":tick.frame,"events":events,
                "retired":reload,"epoch_endpoint_ns":endpoint.as_nanos().to_string()}),
        )?;
        self.frame(tick, exit)
    }

    fn reject_elapsed(&mut self, tick: &LiveTick, error: String) -> String {
        let row = super::diagnostics::elapsed_rejected(tick, &error);
        if let Err(failure) =
            crate::interactive_evidence::journal(&self.playback.request.artifact_root, &row)
        {
            self.diagnostic_failure(format!(
                "strict_elapsed_rejected diagnostic write failed: {failure}"
            ));
        }
        error
    }

    pub(super) fn diagnostic_failure(&mut self, message: String) {
        if self.playback.report.diagnostics.len() < 64 {
            self.playback
                .report
                .diagnostics
                .push(compat_kit::model::Diagnostic {
                    status: compat_kit::model::Status::Error,
                    message,
                });
        } else {
            eprintln!("compat-player diagnostic: {message}");
        }
    }

    fn strict_submit(&mut self, events: &Value) -> Result<(), String> {
        if !events.as_array().is_some_and(Vec::is_empty) {
            self.playback.active()?.input_events(events)?;
        }
        self.strict
            .as_mut()
            .ok_or("strict ingress missing")?
            .accept();
        Ok(())
    }

    fn strict_continuous(
        &mut self,
        tick: &LiveTick,
        steps: &[Duration],
        commands: &[Command],
        reload: bool,
    ) -> Result<bool, String> {
        let stop = commands.contains(&Command::Stop);
        let admitted = if stop || reload { &[][..] } else { steps };
        let advance = worldrunner::scene::viewer::cpu_profile::CpuProfile::span("native_advance");
        for dt in admitted {
            self.playback
                .active()?
                .advance_rational(u64::from(dt.subsec_nanos()), 1_000_000_000)?;
        }
        drop(advance);
        let state = self
            .continuous
            .as_ref()
            .ok_or("interactive state missing")?;
        let finished = stop || state.finished(tick);
        let ratios: Vec<_> = admitted
            .iter()
            .map(|dt| json!({"num":u64::from(dt.subsec_nanos()),"den":1_000_000_000}))
            .collect();
        crate::interactive_evidence::journal(
            &self.playback.request.artifact_root,
            &json!({"stage":"update","execution_profile":crate::strict::PROFILE,"frame":tick.frame,
                "elapsed_ns":tick.elapsed.as_nanos().to_string(),"dt_ns":tick.dt.as_nanos().to_string(),
                "steps":ratios,"reload":reload,"stop":finished,"input":format!("{:?}",tick.input)}),
        )?;
        if tick.frame == 0 || finished || commands.contains(&Command::Capture) {
            self.capture(tick.frame)?;
        }
        Ok(finished)
    }
}

fn host_event(event: &LiveInputEvent) -> bool {
    match event {
        LiveInputEvent::Key { key, .. } => {
            matches!(key.to_lowercase().as_str(), "f5" | "f12" | "escape")
        }
        LiveInputEvent::Focused(_)
        | LiveInputEvent::CloseRequested
        | LiveInputEvent::CaptureRequested(_) => true,
        _ => false,
    }
}
