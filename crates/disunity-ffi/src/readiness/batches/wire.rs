use super::exact::Exact;
use crate::{
    DisunityWorld,
    readiness::Callback,
    strict::{Context, Phase},
};
use serde::{
    Deserialize, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub binding: Exact,
    pub component: Exact,
    pub kind: Callback,
    pub payload: Payload,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Payload {}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lease {
    world: Exact,
    session: String,
    frame: Exact,
    fixed_tick: Exact,
    phase: Phase,
    sequence: Exact,
    token: String,
}
impl From<&Context> for Lease {
    fn from(c: &Context) -> Self {
        Self {
            world: Exact(c.world),
            session: c.session.clone(),
            frame: Exact(c.frame),
            fixed_tick: Exact(c.fixed_tick),
            phase: c.phase,
            sequence: Exact(c.sequence),
            token: c.token.clone(),
        }
    }
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Time {
    numerator: Exact,
    denominator: Exact,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Projection {
    lease: Lease,
    root_phase: Phase,
    phase: String,
    flags: u32,
    time: Time,
    dynamic_delta: f32,
    fixed_delta: f32,
    unscaled_delta: f32,
    pub callback_batch: Exact,
}
impl Projection {
    pub fn new(w: &DisunityWorld, c: &Context, id: u64) -> Result<Self, String> {
        let t = w
            .clock
            .rational
            .as_ref()
            .ok_or("batch requires exact native timeline")?;
        Ok(Self {
            lease: c.into(),
            root_phase: c.phase(),
            phase: "destroy".into(),
            flags: 1,
            time: Time {
                numerator: Exact(t.time.num),
                denominator: Exact(t.time.den),
            },
            dynamic_delta: w.clock.delta_time,
            fixed_delta: w.clock.fixed_delta_time,
            unscaled_delta: w.clock.unscaled_delta_time,
            callback_batch: Exact(id),
        })
    }
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Envelope {
    pub version: u32,
    pub context: Projection,
    #[serde(deserialize_with = "records")]
    pub callbacks: Vec<Record>,
}
fn records<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Record>, D::Error> {
    d.deserialize_seq(Records)
}
struct Records;
impl<'de> Visitor<'de> for Records {
    type Value = Vec<Record>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("at most 4096 callback records")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut records = Vec::new();
        while records.len() < super::super::LIMIT {
            let Some(record) = a.next_element()? else {
                return Ok(records);
            };
            records.push(record);
        }
        if a.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::custom("callback record limit exceeded"));
        }
        Ok(records)
    }
}

impl PartialEq for Projection {
    fn eq(&self, other: &Self) -> bool {
        self.lease == other.lease
            && self.root_phase == other.root_phase
            && self.phase == other.phase
            && self.flags == other.flags
            && self.time == other.time
            && self.callback_batch == other.callback_batch
            && self.dynamic_delta.to_bits() == other.dynamic_delta.to_bits()
            && self.fixed_delta.to_bits() == other.fixed_delta.to_bits()
            && self.unscaled_delta.to_bits() == other.unscaled_delta.to_bits()
    }
}
