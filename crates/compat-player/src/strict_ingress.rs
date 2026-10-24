use serde_json::{Value, json};
use std::time::Duration;
use worldrunner::scene::viewer::{IngressOrigin, LiveInputEvent, LiveInputSample};

#[derive(Default)]
pub(crate) struct Staged {
    queue: Vec<LiveInputSample>,
    epoch: Duration,
    sequence_origin: u64,
    last_sequence: u64,
    last_timestamp: Duration,
    domain: Option<IngressOrigin>,
}
impl Staged {
    pub fn stage(&mut self, sample: &LiveInputSample) -> Result<(), String> {
        if self.queue.len() >= 4096 {
            return Err("strict ingress exceeds 4096 staged observations".into());
        }
        self.queue.push(sample.clone());
        Ok(())
    }

    pub fn batch(&self) -> Result<Value, String> {
        let mut sequence = self.last_sequence.max(self.sequence_origin);
        let mut timestamp = self.last_timestamp;
        let mut domain = self.domain;
        let mut events = Vec::new();
        for sample in &self.queue {
            if sample.sequence <= sequence || sample.timestamp < timestamp {
                return Err("strict ingress IDs/timestamps moved backwards".into());
            }
            if sample.origin == IngressOrigin::ChannelDelivery
                || domain.is_some_and(|domain| domain != sample.origin)
            {
                return Err(
                    "strict ingress requires one OS or replay domain; channel mapping unavailable"
                        .into(),
                );
            }
            sequence = sample.sequence;
            timestamp = sample.timestamp;
            domain = Some(sample.origin);
            if let Some(event) = self.normalize(sample)? {
                events.push(event);
            }
        }
        let events = json!(events);
        if !events.as_array().is_some_and(Vec::is_empty) {
            crate::strict::events(&events)?;
        }
        Ok(events)
    }

    fn normalize(&self, sample: &LiveInputSample) -> Result<Option<Value>, String> {
        let timestamp = sample
            .timestamp
            .checked_sub(self.epoch)
            .ok_or("strict ingress precedes current session epoch")?;
        let num = u64::try_from(timestamp.as_nanos()).map_err(|error| error.to_string())?;
        crate::strict::ratio(&json!({"num":num,"den":1_000_000_000}))?;
        let mut event = match &sample.event {
            LiveInputEvent::Key { key, pressed, .. } => {
                if matches!(key.to_lowercase().as_str(), "f5" | "f12" | "escape") {
                    return Ok(None);
                }
                json!({"kind":"key","key":crate::resources::key_code(key),"pressed":pressed})
            }
            LiveInputEvent::Focused(focused) => json!({"kind":"focus","focused":focused}),
            _ => return Ok(None),
        };
        event["event_id"] = json!(sample.sequence - self.sequence_origin);
        event["timestamp"] = json!({"num":num,"den":1_000_000_000});
        event["source"] = json!(if sample.origin == IngressOrigin::OsEventLoop {
            "os"
        } else {
            "replay"
        });
        Ok(Some(event))
    }

    pub fn accept(&mut self) {
        if let Some(sample) = self.queue.last() {
            self.last_sequence = sample.sequence;
            self.last_timestamp = sample.timestamp;
            self.domain = Some(sample.origin);
        }
        self.queue.clear();
    }

    pub fn retire(&mut self, epoch: Duration) {
        self.sequence_origin = self
            .queue
            .iter()
            .map(|sample| sample.sequence)
            .max()
            .unwrap_or(self.last_sequence)
            .max(self.last_sequence);
        self.queue.clear();
        self.epoch = epoch;
        self.last_sequence = 0;
        self.last_timestamp = epoch;
        self.domain = None;
    }
}

#[cfg(test)]
#[path = "strict_ingress_tests.rs"]
mod tests;
