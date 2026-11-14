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
    targets: Vec<Target>,
}
struct Target {
    bus: usize,
    source_count: usize,
    envelope: Envelope,
}

impl Rule {
    fn refresh_tiers(&mut self, buses: &[Bus]) {
        self.sources
            .sort_unstable_by_key(|&source| std::cmp::Reverse(buses[source].config.tier));
        for target in &mut self.targets {
            target.source_count = self.sources.partition_point(|&source| {
                buses[source].config.tier > buses[target.bus].config.tier
            });
        }
    }
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
            let tier_changed = bus.config.tier != config.tier;
            bus.config = config;
            if tier_changed {
                for rule in &mut self.rules {
                    rule.refresh_tiers(&self.buses);
                }
            }
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
            .map(|bus| Target {
                bus,
                source_count: 0,
                envelope: Envelope::default(),
            })
            .collect();
        let times = rule_times(&config, rate);
        let mut rule = Rule {
            config,
            sources,
            times,
            targets,
        };
        rule.refresh_tiers(&self.buses);
        Ok(rule)
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
            let demands = source_demands(&self.buses, &rule.sources, config.detector);
            for target in &mut rule.targets {
                let demand = demands[target.source_count];
                let depth = target.envelope.advance(demand, rule.times);
                let gain = gain_at(config.envelope, config.gain, depth);
                let bus = &mut self.buses[target.bus];
                bus.duck = bus.duck.min(gain);
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
            for target in &mut rule.targets {
                target.envelope = Envelope::default();
            }
        }
    }
}

fn source_demands(
    buses: &[Bus],
    sources: &[usize],
    detector: DuckingDetector,
) -> [f32; MAX_AUDIO_BUSES + 1] {
    // Descending tiers make each target's eligible sources a prefix, excluding equal tiers.
    let mut demands = [0.0_f32; MAX_AUDIO_BUSES + 1];
    for (position, &source) in sources.iter().enumerate() {
        let demand = match detector {
            DuckingDetector::Activity => {
                if buses[source].active {
                    1.0
                } else {
                    0.0
                }
            }
            DuckingDetector::Peak => buses[source].peak,
        };
        demands[position + 1] = demands[position].max(demand);
    }
    demands
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_demands_match_direct_tier_scan_bit_for_bit() {
        let mut routing = Routing::default();
        let names: Vec<_> = (0..MAX_AUDIO_BUSES).map(|i| format!("b{i}")).collect();
        let tiers = [i32::MIN, 0, -1, 0, i32::MAX, 1];
        for (i, name) in names.iter().enumerate() {
            routing
                .configure(
                    name,
                    BusConfig {
                        gain: 1.0,
                        muted: false,
                        tier: tiers[i % tiers.len()],
                    },
                )
                .unwrap();
        }
        for detector in [DuckingDetector::Activity, DuckingDetector::Peak] {
            routing
                .replace(
                    vec![DuckingRule {
                        source_buses: names.iter().step_by(2).cloned().collect(),
                        target_buses: names.clone(),
                        gain: 0.25,
                        attack: Duration::ZERO,
                        hold: Duration::ZERO,
                        release: Duration::ZERO,
                        envelope: EnvelopeMode::LinearGain,
                        detector,
                    }],
                    48_000,
                )
                .unwrap();
            for changed in [0, 4, 31, 63] {
                let mut config = routing.buses[changed].config;
                config.tier = i32::MAX;
                routing.configure(&names[changed], config).unwrap();
                routing.begin_frame();
                for i in (0..MAX_AUDIO_BUSES).step_by(3) {
                    routing.observe(i, [f32::from(u16::try_from(i).unwrap()) / 64.0; 2]);
                }
                let rule = &routing.rules[0];
                let demands = source_demands(&routing.buses, &rule.sources, detector);
                for target in &rule.targets {
                    let direct = routing
                        .buses
                        .iter()
                        .enumerate()
                        .filter(|(i, bus)| {
                            i % 2 == 0 && bus.config.tier > routing.buses[target.bus].config.tier
                        })
                        .map(|(_, bus)| match detector {
                            DuckingDetector::Activity => {
                                if bus.active {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                            DuckingDetector::Peak => bus.peak,
                        })
                        .fold(0.0_f32, f32::max);
                    assert_eq!(demands[target.source_count].to_bits(), direct.to_bits());
                }
            }
        }
    }
}
