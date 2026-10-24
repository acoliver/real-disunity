use crate::{
    DisunityWorld,
    ratio::Ratio,
    strict::{self, Context, Host, Phase},
};
use serde::Deserialize;
use serde_json::Value;
#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum StrictRequest {
    #[serde(rename = "session.open")]
    Open {
        profile: String,
        #[serde(skip)]
        seed: Option<String>,
    },
    #[serde(rename = "host.mutate_idle")]
    MutateIdle {
        host: Host,
        request: strict::IdleRename,
    },
    #[serde(rename = "session.configure")]
    Configure {
        host: Host,
        time_scale: Option<Ratio>,
        fixed_delta_time: Option<Ratio>,
    },
    #[serde(rename = "session.begin_cleanup")]
    BeginCleanup {
        host: Host,
        reason: Option<strict::CleanupReason>,
        #[serde(skip)]
        seed: Option<String>,
    },
    #[serde(rename = "session.close")]
    Close { host: Host },
    #[serde(rename = "frame.open")]
    Begin { host: Host, dt: Ratio },
    #[serde(rename = "phase.open")]
    Phase {
        host: Host,
        phase: Phase,
        frame: u64,
        fixed_tick: u64,
        #[serde(skip)]
        seed: Option<String>,
    },
    #[serde(rename = "phase.claim")]
    Claim { phase: Phase, context: Context },
    #[serde(rename = "phase.finish")]
    Finish { host: Host, context: Context },
    #[serde(rename = "phase.cancel")]
    Cancel { host: Host, context: Context },
    #[serde(rename = "fixed.commit")]
    Commit {
        host: Host,
        frame: u64,
        fixed_tick: u64,
    },
    #[serde(rename = "frame.end")]
    End { host: Host, frame: u64 },
    #[serde(rename = "frame.abort")]
    Abort { host: Host, frame: u64 },
}
impl StrictRequest {
    pub(super) fn prepare(&mut self) -> Result<(), String> {
        match self {
            Self::Open { seed, .. } => *seed = Some(strict::entropy()? + &strict::entropy()?),
            Self::Phase { seed, .. } | Self::BeginCleanup { seed, .. } => {
                *seed = Some(strict::entropy()?)
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn execute(self, w: &mut DisunityWorld, world: u64) -> Result<Value, String> {
        match self {
            Self::Open { profile, seed } => {
                strict::open(w, world, &profile, seed.ok_or("missing native entropy")?)
            }
            Self::MutateIdle { host, request } => strict::mutate_idle(w, world, &host, request),
            Self::Configure {
                host,
                time_scale,
                fixed_delta_time,
            } => strict::configure(w, &host, time_scale, fixed_delta_time),
            Self::BeginCleanup { host, reason, seed } => {
                strict::begin_cleanup(w, &host, reason, seed.ok_or("missing native entropy")?)
            }
            Self::Close { host } => strict::close(w, &host),
            Self::Begin { host, dt } => strict::begin(w, &host, dt),
            Self::Phase {
                host,
                phase,
                frame,
                fixed_tick,
                seed,
            } => strict::issue(
                w,
                &host,
                phase,
                (frame, fixed_tick),
                seed.ok_or("missing native entropy")?,
            ),
            Self::Claim { phase, context } => strict::claim(w, world, phase, context),
            Self::Finish { host, context } => strict::finish(w, &host, context),
            Self::Cancel { host, context } => strict::cancel(w, &host, context),
            Self::Commit {
                host,
                frame,
                fixed_tick,
            } => strict::commit(w, &host, frame, fixed_tick),
            Self::End { host, frame } => strict::end(w, &host, frame),
            Self::Abort { host, frame } => strict::abort(w, &host, frame),
        }
    }
}
