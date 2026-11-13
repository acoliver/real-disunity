use super::{
    AudioError,
    envelope::{Envelope, duration_frames, gain_at},
    validate_gain,
};
use std::{str::FromStr, time::Duration};

pub const MAX_AUDIO_BUSES: usize = 64;
pub const MAX_DUCKING_RULES: usize = 64;
pub const MAX_BUS_NAME_BYTES: usize = 128;
pub const MAX_DUCKING_TIME: Duration = Duration::from_hours(1);

/// Higher numeric tiers take precedence. Equal tiers never duck one another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BusConfig {
    pub gain: f32,
    pub muted: bool,
    pub tier: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeMode {
    LinearGain,
    LinearDb,
}
impl FromStr for EnvelopeMode {
    type Err = AudioError;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "linear-gain" => Ok(Self::LinearGain),
            "linear-db" => Ok(Self::LinearDb),
            _ => Err(AudioError::UnsupportedEnvelope(name.into())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuckingDetector {
    /// Any playing source voice, including silent or muted voices.
    Activity,
    /// Maximum absolute source sample, after voice/bus gain and mute, before ducking.
    Peak,
}

/// One reduction factor and envelope shared by the named source/target sets.
/// Each target uses only sources at a strictly higher numeric tier.
#[derive(Clone, Debug, PartialEq)]
pub struct DuckingRule {
    pub source_buses: Vec<String>,
    pub target_buses: Vec<String>,
    /// Fully ducked gain factor in 0..=1; linear-db requires a positive factor.
    pub gain: f32,
    pub attack: Duration,
    pub hold: Duration,
    pub release: Duration,
    pub envelope: EnvelopeMode,
    pub detector: DuckingDetector,
}

struct Bus {
    name: String,
    config: BusConfig,
    active: bool,
    peak: f32,
    duck: f32,
}
impl Bus {
    fn gain(&self) -> f32 {
        if self.config.muted {
            0.0
        } else {
            self.config.gain
        }
    }
}
struct Rule {
    config: DuckingRule,
    sources: Vec<usize>,
    times: [u32; 3],
    targets: Vec<(usize, Envelope)>,
}

#[derive(Default)]
pub(super) struct Routing {
    buses: Vec<Bus>,
    rules: Vec<Rule>,
}
impl Routing {
    pub fn index(&self, name: &str) -> Result<usize, AudioError> {
        self.buses
            .iter()
            .position(|bus| bus.name == name)
            .ok_or_else(|| AudioError::UnknownBus(name.into()))
    }
    pub fn configure(&mut self, name: &str, config: BusConfig) -> Result<(), AudioError> {
        if name.is_empty() || name.len() > MAX_BUS_NAME_BYTES {
            return Err(AudioError::InvalidBusName(name.into()));
        }
        validate_gain(config.gain)?;
        if let Some(bus) = self.buses.iter_mut().find(|bus| bus.name == name) {
            bus.config = config;
        } else {
            if self.buses.len() == MAX_AUDIO_BUSES {
                return Err(AudioError::BusLimit);
            }
            self.buses.push(Bus {
                name: name.into(),
                config,
                active: false,
                peak: 0.0,
                duck: 1.0,
            });
        }
        Ok(())
    }
    pub fn replace(&mut self, configs: Vec<DuckingRule>, rate: u32) -> Result<(), AudioError> {
        if configs.len() > MAX_DUCKING_RULES {
            return Err(AudioError::DuckingRuleLimit);
        }
        let rules = configs
            .into_iter()
            .map(|config| self.resolve_rule(config, rate))
            .collect::<Result<_, _>>()?;
        self.rules = rules;
        Ok(())
    }
    fn resolve_rule(&self, config: DuckingRule, rate: u32) -> Result<Rule, AudioError> {
        validate_rule(&config)?;
        let sources = self.resolve_names(&config.source_buses)?;
        let targets = self
            .resolve_names(&config.target_buses)?
            .into_iter()
            .map(|index| (index, Envelope::default()))
            .collect();
        let times = rule_times(&config, rate);
        Ok(Rule {
            config,
            sources,
            times,
            targets,
        })
    }
    fn resolve_names(&self, names: &[String]) -> Result<Vec<usize>, AudioError> {
        if names.is_empty() || names.len() > MAX_AUDIO_BUSES {
            return Err(AudioError::InvalidDuckingRule);
        }
        let mut indices = names
            .iter()
            .map(|name| self.index(name))
            .collect::<Result<Vec<_>, _>>()?;
        indices.sort_unstable();
        indices.dedup();
        Ok(indices)
    }
    pub fn begin_frame(&mut self) {
        for bus in &mut self.buses {
            bus.active = false;
            bus.peak = 0.0;
            bus.duck = 1.0;
        }
    }
    pub fn observe(&mut self, index: usize, sample: [f32; 2]) {
        let bus = &mut self.buses[index];
        bus.active = true;
        let peak = sample[0].abs().max(sample[1].abs()) * bus.gain();
        bus.peak = bus.peak.max(peak.min(1.0));
    }
    pub fn advance(&mut self) {
        for rule in &mut self.rules {
            let config = &rule.config;
            for (target, envelope) in &mut rule.targets {
                let demand = source_demand(&self.buses, &rule.sources, *target, config.detector);
                let depth = envelope.advance(demand, rule.times);
                let gain = gain_at(config.envelope, config.gain, depth);
                self.buses[*target].duck = self.buses[*target].duck.min(gain);
            }
        }
    }
    pub fn gain(&self, index: usize) -> f32 {
        let bus = &self.buses[index];
        bus.gain() * bus.duck
    }
    pub fn reset(&mut self, rate: u32) {
        for rule in &mut self.rules {
            rule.times = rule_times(&rule.config, rate);
            for (_, envelope) in &mut rule.targets {
                *envelope = Envelope::default();
            }
        }
    }
}

fn source_demand(
    buses: &[Bus],
    sources: &[usize],
    target: usize,
    detector: DuckingDetector,
) -> f32 {
    sources
        .iter()
        .filter(|&&source| buses[source].config.tier > buses[target].config.tier)
        .map(|&source| match detector {
            DuckingDetector::Activity => {
                if buses[source].active {
                    1.0
                } else {
                    0.0
                }
            }
            DuckingDetector::Peak => buses[source].peak,
        })
        .fold(0.0, f32::max)
}

fn validate_rule(rule: &DuckingRule) -> Result<(), AudioError> {
    if !rule.gain.is_finite()
        || !(0.0..=1.0).contains(&rule.gain)
        || (rule.envelope == EnvelopeMode::LinearDb && rule.gain == 0.0)
        || [rule.attack, rule.hold, rule.release]
            .iter()
            .any(|time| *time > MAX_DUCKING_TIME)
    {
        return Err(AudioError::InvalidDuckingRule);
    }
    Ok(())
}

fn rule_times(config: &DuckingRule, rate: u32) -> [u32; 3] {
    [config.attack, config.hold, config.release].map(|time| duration_frames(time, rate))
}
