use super::support::{bus, clip, options, render};
use std::time::Duration;
use worldrunner_world_simulation::audio::{
    AudioMixer, DuckingDetector, DuckingRule, EnvelopeMode, ExistingVoices,
};

#[test]
fn suspended_sources_leave_detectors_while_envelopes_keep_output_time() {
    for detector in [DuckingDetector::Activity, DuckingDetector::Peak] {
        let mut mixer = AudioMixer::new(8).unwrap();
        bus(&mut mixer, "target", 1.0, false, 0);
        bus(&mut mixer, "source", 1.0, false, 1);
        let target = clip(&mut mixer, &[0.5; 64], 8);
        let source = clip(&mut mixer, &[1.0; 64], 8);
        mixer
            .replace_ducking_rules(vec![DuckingRule {
                source_buses: vec!["source".into()],
                target_buses: vec!["target".into()],
                gain: 0.25,
                attack: Duration::ZERO,
                hold: Duration::from_millis(250),
                release: Duration::from_millis(500),
                envelope: EnvelopeMode::LinearGain,
                detector,
            }])
            .unwrap();
        let mut source_options = options(Some("source"), true);
        source_options.gain = if detector == DuckingDetector::Activity {
            0.0
        } else {
            1.0
        };
        mixer
            .play_with_options(&source, source_options.clone())
            .unwrap();
        mixer.play_on_bus(&target, 1.0, "target").unwrap();
        let first = render(&mut mixer, 1);
        assert_eq!(
            first,
            [[if detector == DuckingDetector::Activity {
                0.125
            } else {
                1.0
            }; 2]]
        );
        mixer.pause(1, ExistingVoices::Suspend);
        assert_eq!(render(&mut mixer, 6), [[0.0; 2]; 6]);
        mixer
            .play_with_options(&target, options(Some("target"), false))
            .unwrap();
        assert_eq!(render(&mut mixer, 1), [[0.5; 2]]);
        source_options.pause_eligible = false;
        let independent = mixer.play_with_options(&source, source_options).unwrap();
        let active = render(&mut mixer, 1);
        assert_eq!(
            active,
            [[if detector == DuckingDetector::Activity {
                0.125
            } else {
                1.0
            }; 2]]
        );
        mixer.stop(&independent);
        assert_eq!(
            render(&mut mixer, 6),
            [
                [0.125; 2],
                [0.125; 2],
                [0.21875; 2],
                [0.3125; 2],
                [0.40625; 2],
                [0.5; 2]
            ]
        );
        mixer.resume(1);
        assert_eq!(
            render(&mut mixer, 1),
            [[if detector == DuckingDetector::Activity {
                0.25
            } else {
                1.0
            }; 2]]
        );
    }
}
