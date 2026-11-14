use std::time::Duration;
use worldrunner_world_simulation::audio::{
    AudioCommand, AudioError, AudioMixer, AudioReply, BusConfig, DuckingDetector, DuckingRule,
    EnvelopeMode,
};

fn clip(
    mixer: &mut AudioMixer,
    samples: &[f32],
    rate: u32,
) -> worldrunner_world_simulation::audio::ClipHandle {
    let data: Vec<_> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();
    let size = u32::try_from(data.len()).unwrap();
    let mut wav = b"RIFF".to_vec();
    wav.extend((36 + size).to_le_bytes());
    wav.extend(b"WAVEfmt \x10\0\0\0\x03\0\x01\0");
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 4).to_le_bytes());
    wav.extend([4, 0, 32, 0]);
    wav.extend(b"data");
    wav.extend(size.to_le_bytes());
    wav.extend(data);
    mixer.load_wav(&wav).unwrap()
}

fn bus(mixer: &mut AudioMixer, name: &str, tier: i32) {
    mixer
        .configure_bus(
            name,
            BusConfig {
                gain: 1.0,
                muted: false,
                tier,
            },
        )
        .unwrap();
}

fn rule(source: &str, target: &str) -> DuckingRule {
    DuckingRule {
        source_buses: vec![source.into()],
        target_buses: vec![target.into()],
        gain: 0.25,
        attack: Duration::from_millis(500),
        hold: Duration::from_millis(250),
        release: Duration::from_millis(500),
        envelope: EnvelopeMode::LinearGain,
        detector: DuckingDetector::Activity,
    }
}

fn samples(mixer: &mut AudioMixer, count: usize) -> Vec<f32> {
    let mut out = vec![[0.0; 2]; count];
    mixer.render(&mut out);
    assert!(out.iter().all(|v| v[0].to_bits() == v[1].to_bits()));
    out.into_iter().map(|v| v[0]).collect()
}

fn setup() -> (AudioMixer, worldrunner_world_simulation::audio::ClipHandle) {
    let mut mixer = AudioMixer::new(8).unwrap();
    bus(&mut mixer, "music", 0);
    bus(&mut mixer, "speech", 1);
    let clip = clip(&mut mixer, &[0.5; 64], 8);
    (mixer, clip)
}

#[test]
fn routing_bus_gain_mute_and_master_apply_to_every_voice() {
    let (mut mixer, clip) = setup();
    mixer
        .configure_bus(
            "music",
            BusConfig {
                gain: 0.5,
                muted: false,
                tier: 0,
            },
        )
        .unwrap();
    mixer.play_on_bus(&clip, 0.5, "music").unwrap();
    mixer.play_on_bus(&clip, 0.5, "music").unwrap();
    let speech = mixer.play_on_bus(&clip, 0.25, "speech").unwrap();
    mixer.play(&clip, 0.125).unwrap();
    mixer.set_master_gain(0.5).unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.21875]);
    mixer
        .configure_bus(
            "music",
            BusConfig {
                gain: 0.5,
                muted: true,
                tier: 0,
            },
        )
        .unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.09375]);
    assert!(mixer.stop(&speech));
    assert_eq!(samples(&mut mixer, 1), [0.03125]);
    mixer
        .configure_bus(
            "music",
            BusConfig {
                gain: 1.0,
                muted: false,
                tier: 0,
            },
        )
        .unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.28125]);
}

#[test]
fn unknown_bus_and_mode_errors_name_the_input() {
    let (mut mixer, clip) = setup();
    assert_eq!(
        mixer.play_on_bus(&clip, 1.0, "typo"),
        Err(AudioError::UnknownBus("typo".into()))
    );
    assert_eq!(mixer.active_voices(), 0);
    assert_eq!(
        "linear-gain".parse::<EnvelopeMode>(),
        Ok(EnvelopeMode::LinearGain)
    );
    assert_eq!(
        "linear-db".parse::<EnvelopeMode>(),
        Ok(EnvelopeMode::LinearDb)
    );
    assert_eq!(
        "cubic".parse::<EnvelopeMode>(),
        Err(AudioError::UnsupportedEnvelope("cubic".into()))
    );
}

#[test]
fn attack_hold_release_are_exact_and_block_partition_independent() {
    for partition in [1, 3, 20] {
        let (mut mixer, clip) = setup();
        mixer
            .replace_ducking_rules(vec![rule("speech", "music")])
            .unwrap();
        mixer.play_on_bus(&clip, 1.0, "music").unwrap();
        let source = clip_for_source(&mut mixer, 4);
        mixer.play_on_bus(&source, 0.0, "speech").unwrap();
        let mut actual = Vec::new();
        while actual.len() < 12 {
            let n = partition.min(12 - actual.len());
            actual.extend(samples(&mut mixer, n));
        }
        assert_eq!(
            actual,
            [
                0.40625, 0.3125, 0.21875, 0.125, 0.125, 0.125, 0.21875, 0.3125, 0.40625, 0.5, 0.5,
                0.5
            ]
        );
    }
}

fn clip_for_source(
    mixer: &mut AudioMixer,
    frames: usize,
) -> worldrunner_world_simulation::audio::ClipHandle {
    clip(mixer, &vec![0.5; frames], 8)
}

#[test]
fn replacement_is_atomic_and_removes_old_envelopes() {
    let (mut mixer, clip) = setup();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    let mut r = rule("speech", "music");
    r.attack = Duration::ZERO;
    mixer.replace_ducking_rules(vec![r.clone()]).unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.125]);
    let mut invalid = r.clone();
    invalid.target_buses = vec!["absent".into()];
    assert_eq!(
        mixer.replace_ducking_rules(vec![invalid]),
        Err(AudioError::UnknownBus("absent".into()))
    );
    assert_eq!(samples(&mut mixer, 1), [0.125]);
    r.gain = 0.5;
    mixer.replace_ducking_rules(vec![r]).unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.25]);
    mixer.replace_ducking_rules(vec![]).unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.5]);
}

#[test]
fn tiers_gate_sources_and_strongest_rule_wins_without_stacking() {
    let (mut mixer, clip) = setup();
    bus(&mut mixer, "alert", 2);
    let mut low = rule("music", "speech");
    low.attack = Duration::ZERO;
    let mut middle = rule("speech", "music");
    middle.attack = Duration::ZERO;
    middle.gain = 0.5;
    let mut high = rule("alert", "music");
    high.attack = Duration::ZERO;
    mixer
        .replace_ducking_rules(vec![low, middle, high])
        .unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.125, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.3125]);
    let alert = mixer.play_on_bus(&clip, 0.0, "alert").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.1875]);
    mixer.stop(&alert);
    assert_eq!(samples(&mut mixer, 2), [0.1875; 2]);
    assert_eq!(samples(&mut mixer, 1), [0.28125]);
}

#[test]
fn linear_db_envelope_and_peak_detector_are_distinct_controls() {
    let (mut mixer, clip) = setup();
    let mut r = rule("speech", "music");
    r.envelope = EnvelopeMode::LinearDb;
    mixer.replace_ducking_rules(vec![r.clone()]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    let source = mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    let out = samples(&mut mixer, 4);
    for (actual, expected) in out.iter().zip([
        0.5 * 0.25_f32.powf(0.25),
        0.25,
        0.5 * 0.25_f32.powf(0.75),
        0.125,
    ]) {
        assert!((actual - expected).abs() < 1e-6);
    }
    mixer.stop(&source);
    r.envelope = EnvelopeMode::LinearGain;
    r.detector = DuckingDetector::Peak;
    r.attack = Duration::ZERO;
    mixer.replace_ducking_rules(vec![r]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.8125]);
}

#[test]
fn typed_commands_cover_routing_and_configuration() {
    let mut mixer = AudioMixer::new(8).unwrap();
    assert_eq!(
        mixer.command(AudioCommand::ConfigureBus {
            name: "fx".into(),
            config: BusConfig {
                gain: 0.5,
                muted: false,
                tier: 0
            }
        }),
        Ok(AudioReply::Applied)
    );
    assert_eq!(
        mixer.command(AudioCommand::DuckingRules(vec![])),
        Ok(AudioReply::Applied)
    );
    let clip = clip(&mut mixer, &[0.5], 8);
    assert!(matches!(
        mixer.command(AudioCommand::PlayOnBus {
            clip,
            gain: 1.0,
            bus: "fx".into()
        }),
        Ok(AudioReply::Playing(_))
    ));
    assert_eq!(samples(&mut mixer, 1), [0.25]);
}

#[test]
fn unrouted_mixing_matches_previous_sample_arithmetic_bit_for_bit() {
    let source = [0.125_f32, -0.6, 0.8, 0.4, -0.2];
    for (source_rate, output_rate) in [(3, 8), (8, 3), (8, 8), (44100, 48000), (48000, 44100)] {
        let mut mixer = AudioMixer::new(output_rate).unwrap();
        bus(&mut mixer, "unused", 0);
        bus(&mut mixer, "trigger", 1);
        mixer
            .replace_ducking_rules(vec![rule("trigger", "unused")])
            .unwrap();
        let clip = clip(&mut mixer, &source, source_rate);
        let gains = [0.3, 1.125, 0.0625];
        for gain in gains {
            mixer.play(&clip, gain).unwrap();
        }
        mixer.set_master_gain(0.7).unwrap();
        let count = (source.len() * output_rate as usize).div_ceil(source_rate as usize);
        let actual = samples(&mut mixer, count);
        let expected = old_mix(&source, gains, source_rate, output_rate, count);
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            expected
        );
    }
}

// The pre-bus mixer summed interpolated voices directly, then applied master gain.
fn old_mix(source: &[f32], gains: [f32; 3], input: u32, output: u32, count: usize) -> Vec<u32> {
    let mut position = 0_u64;
    let mut result = Vec::new();
    for _ in 0..count {
        let index = usize::try_from(position / u64::from(output)).unwrap();
        let next = (index + 1).min(source.len() - 1);
        let fraction = f32::from(u16::try_from(position % u64::from(output)).unwrap())
            / f32::from(u16::try_from(output).unwrap());
        let mut sum = 0.0;
        for gain in gains {
            sum += (source[index] + (source[next] - source[index]) * fraction) * gain;
        }
        result.push((sum * 0.7).clamp(-1.0, 1.0).to_bits());
        position += u64::from(input);
    }
    result
}

#[test]
fn envelope_times_follow_output_rate_and_round_up_fractional_frames() {
    for rate in [8, 16] {
        let mut mixer = AudioMixer::new(rate).unwrap();
        bus(&mut mixer, "music", 0);
        bus(&mut mixer, "speech", 1);
        let clip = clip(&mut mixer, &[0.5; 64], rate);
        mixer.play_on_bus(&clip, 1.0, "music").unwrap();
        mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
        mixer
            .replace_ducking_rules(vec![rule("speech", "music")])
            .unwrap();
        let n = usize::try_from(rate / 2).unwrap();
        let out = samples(&mut mixer, n);
        assert_eq!(out.last(), Some(&0.125));
        assert!(out[n - 2] > 0.125);
    }
    let (mut mixer, clip) = setup();
    let mut r = rule("speech", "music");
    r.attack = Duration::from_millis(126);
    mixer.replace_ducking_rules(vec![r]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 2), [0.3125, 0.125]);
}

#[test]
fn multiple_source_buses_and_voices_hold_until_the_last_source_stops() {
    let (mut mixer, clip) = setup();
    bus(&mut mixer, "alert", 2);
    let mut r = rule("speech", "music");
    r.source_buses.push("alert".into());
    r.attack = Duration::ZERO;
    r.hold = Duration::ZERO;
    mixer.replace_ducking_rules(vec![r]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    let a = mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    let b = mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    let c = mixer.play_on_bus(&clip, 0.0, "alert").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.125]);
    mixer.stop(&a);
    mixer.stop(&c);
    assert_eq!(samples(&mut mixer, 1), [0.125]);
    mixer.stop(&b);
    assert_eq!(samples(&mut mixer, 2), [0.21875, 0.3125]);
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.125]);
}

#[test]
fn invalid_settings_are_rejected_without_modifying_live_buses_or_rules() {
    use worldrunner_world_simulation::audio::{
        MAX_AUDIO_BUSES, MAX_DUCKING_RULES, MAX_DUCKING_TIME,
    };
    let (mut mixer, clip) = setup();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    assert!(matches!(
        mixer.configure_bus(
            "",
            BusConfig {
                gain: 1.0,
                muted: false,
                tier: 0
            }
        ),
        Err(AudioError::InvalidBusName(_))
    ));
    for gain in [f32::NAN, f32::INFINITY, -0.1, 16.1] {
        assert_eq!(
            mixer.configure_bus(
                "music",
                BusConfig {
                    gain,
                    muted: false,
                    tier: 0
                }
            ),
            Err(AudioError::InvalidGain)
        );
    }
    assert_eq!(samples(&mut mixer, 1), [0.5]);
    let mut r = rule("speech", "music");
    for gain in [f32::NAN, -0.1, 1.1] {
        r.gain = gain;
        assert_eq!(
            mixer.replace_ducking_rules(vec![r.clone()]),
            Err(AudioError::InvalidDuckingRule)
        );
    }
    r.gain = 0.0;
    r.envelope = EnvelopeMode::LinearDb;
    assert_eq!(
        mixer.replace_ducking_rules(vec![r.clone()]),
        Err(AudioError::InvalidDuckingRule)
    );
    r.gain = 0.5;
    r.attack = MAX_DUCKING_TIME + Duration::from_nanos(1);
    assert_eq!(
        mixer.replace_ducking_rules(vec![r]),
        Err(AudioError::InvalidDuckingRule)
    );
    assert_eq!(
        mixer.replace_ducking_rules(vec![rule("speech", "music"); MAX_DUCKING_RULES + 1]),
        Err(AudioError::DuckingRuleLimit)
    );
    for n in 2..MAX_AUDIO_BUSES {
        bus(&mut mixer, &format!("bus{n}"), 0);
    }
    assert_eq!(
        mixer.configure_bus(
            "overflow",
            BusConfig {
                gain: 1.0,
                muted: false,
                tier: 0
            }
        ),
        Err(AudioError::BusLimit)
    );
}

#[test]
fn peak_sidechain_uses_option_gain_and_mute_before_ducking() {
    let (mut mixer, clip) = setup();
    let mut r = rule("speech", "music");
    r.detector = DuckingDetector::Peak;
    r.attack = Duration::ZERO;
    r.hold = Duration::ZERO;
    r.release = Duration::ZERO;
    mixer.replace_ducking_rules(vec![r]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 1.0, "speech").unwrap();
    mixer
        .configure_bus(
            "speech",
            BusConfig {
                gain: 0.5,
                muted: false,
                tier: 1,
            },
        )
        .unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.65625]);
    mixer
        .configure_bus(
            "speech",
            BusConfig {
                gain: 0.5,
                muted: true,
                tier: 1,
            },
        )
        .unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.5]);
}

#[test]
fn partial_attack_holds_then_releases_and_retrigger_starts_at_current_depth() {
    let (mut mixer, clip) = setup();
    mixer
        .replace_ducking_rules(vec![rule("speech", "music")])
        .unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    let source = mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 2), [0.40625, 0.3125]);
    mixer.stop(&source);
    assert_eq!(
        samples(&mut mixer, 4),
        [0.3125, 0.3125, 23.0 / 64.0, 0.40625]
    );
    let source = mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 1), [43.0 / 128.0]);
    mixer.stop(&source);
    assert_eq!(
        samples(&mut mixer, 6),
        [
            43.0 / 128.0,
            43.0 / 128.0,
            193.0 / 512.0,
            107.0 / 256.0,
            235.0 / 512.0,
            0.5
        ]
    );
}

#[test]
fn each_target_filters_source_tiers_independently_including_equal_tiers() {
    let (mut mixer, clip) = setup();
    bus(&mut mixer, "peer", 1);
    let mut r = rule("speech", "music");
    r.target_buses.push("peer".into());
    r.attack = Duration::ZERO;
    mixer.replace_ducking_rules(vec![r]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.5, "peer").unwrap();
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.375]);
}

#[test]
fn accepted_output_reconfiguration_retimes_rules_and_clear_resets_envelopes() {
    let (mut mixer, clip) = setup();
    mixer
        .replace_ducking_rules(vec![rule("speech", "music")])
        .unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 4), [0.40625, 0.3125, 0.21875, 0.125]);
    mixer.clear_voices();
    mixer.set_output_rate(16).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    assert_eq!(samples(&mut mixer, 1), [29.0 / 64.0]);
    assert_eq!(samples(&mut mixer, 7).last(), Some(&0.125));
}

#[test]
fn tier_changes_refresh_lookups_without_resetting_live_envelopes() {
    let (mut mixer, clip) = setup();
    bus(&mut mixer, "alert", 2);
    let mut r = rule("speech", "music");
    r.source_buses.push("alert".into());
    mixer.replace_ducking_rules(vec![r]).unwrap();
    mixer.play_on_bus(&clip, 1.0, "music").unwrap();
    mixer.play_on_bus(&clip, 0.0, "speech").unwrap();
    mixer.play_on_bus(&clip, 0.0, "alert").unwrap();
    assert_eq!(samples(&mut mixer, 1), [0.40625]);
    bus(&mut mixer, "speech", 3);
    assert_eq!(samples(&mut mixer, 1), [0.3125]);
    bus(&mut mixer, "music", 3);
    assert_eq!(samples(&mut mixer, 2), [0.3125; 2]);
    bus(&mut mixer, "alert", 4);
    assert_eq!(samples(&mut mixer, 1), [0.265_625]);
}
