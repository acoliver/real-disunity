use crate::{
    DisunityWorld,
    input::{event::Event, projection::Phase},
    ratio::Ratio,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum TimelineRequest {
    #[serde(rename = "time.set_rational")]
    Configure {
        time_scale: Option<Ratio>,
        fixed_delta_time: Option<Ratio>,
    },
    #[serde(rename = "frame.begin_rational")]
    Begin { dt: Ratio },
    #[serde(rename = "input.events")]
    Events { events: Vec<Event> },
}
impl TimelineRequest {
    pub(super) fn execute(self, w: &mut DisunityWorld) -> Result<Value, String> {
        match self {
            Self::Configure {
                time_scale,
                fixed_delta_time,
            } => w.clock.set_rational(time_scale, fixed_delta_time),
            Self::Begin { dt } => {
                w.clock.begin_rational(dt)?;
                w.input
                    .advance(Phase::Dynamic, w.clock.endpoint()?, w.clock.frame)?;
                Ok(json!(w.clock))
            }
            Self::Events { events } => w.input.enqueue(
                events,
                w.clock.endpoint()?,
                w.clock.frame,
                w.clock.fixed_frame,
            ),
        }
    }
}
