//! CPU-only scene admission, translation and jitter regressions.
use crate::{post, post_taa_tests::effects};
use worldrunner_render_plan as s;
use worldrunner_render_post as p;

#[test]
fn smaa_presets_translate_exactly_with_both_edge_modes_and_no_jitter() {
    for (preset, expected) in [
        (s::SceneSmaaPreset::Low, p::SmaaSettings::LOW),
        (s::SceneSmaaPreset::Medium, p::SmaaSettings::MEDIUM),
        (s::SceneSmaaPreset::High, p::SmaaSettings::HIGH),
        (s::SceneSmaaPreset::Ultra, p::SmaaSettings::ULTRA),
    ] {
        for (edge_detection, expected_edge) in [
            (s::SceneSmaaEdgeDetection::Luma, p::SmaaEdgeDetection::Luma),
            (
                s::SceneSmaaEdgeDetection::Color,
                p::SmaaEdgeDetection::Color,
            ),
        ] {
            let authored = effects(
                s::SceneAntiAlias::Smaa(s::SceneSmaaSettings {
                    edge_detection,
                    ..s::SceneSmaaSettings::preset(preset)
                }),
                None,
            );
            let settings = post::settings(&authored).unwrap();
            assert_eq!(
                settings.resolve,
                p::PostResolve::Smaa(p::SmaaSettings {
                    edge_detection: expected_edge,
                    ..expected
                })
            );
            let config = p::FrameConfig::new([64, 48], wgpu::TextureFormat::Rgba16Float, 1);
            assert_eq!(post::jitter(&settings, config, None), Ok(None));
            assert_eq!(crate::frame::validate_stock_effects(&authored), Ok(()));
        }
    }
}

#[test]
fn smaa_thresholds_round_once_at_boundaries_and_presets() {
    for (threshold_micro, expected) in [
        (49_990, 0.04999_f32),
        (1, 0.000_001),
        (500_000, 0.5),
        (s::SceneSmaaSettings::LOW.threshold_micro, 0.15),
        (s::SceneSmaaSettings::MEDIUM.threshold_micro, 0.1),
        (s::SceneSmaaSettings::HIGH.threshold_micro, 0.1),
        (s::SceneSmaaSettings::ULTRA.threshold_micro, 0.05),
    ] {
        let settings = post::settings(&effects(
            s::SceneAntiAlias::Smaa(s::SceneSmaaSettings {
                threshold_micro,
                ..s::SceneSmaaSettings::HIGH
            }),
            None,
        ))
        .unwrap();
        let p::PostResolve::Smaa(actual) = settings.resolve else {
            panic!("expected SMAA resolve");
        };
        assert_eq!(
            actual.threshold.to_bits(),
            expected.to_bits(),
            "threshold_micro: {threshold_micro}"
        );
    }
}

#[test]
fn smaa_custom_values_reach_post_without_losing_optional_zeroes() {
    for diagonal in [None, Some(0), Some(20)] {
        for corners in [None, Some(0), Some(100)] {
            let settings = post::settings(&effects(
                s::SceneAntiAlias::Smaa(s::SceneSmaaSettings {
                    threshold_micro: 125_000,
                    max_search_steps: 112,
                    max_search_steps_diag: diagonal,
                    corner_rounding: corners,
                    edge_detection: s::SceneSmaaEdgeDetection::Color,
                }),
                None,
            ))
            .unwrap();
            assert_eq!(
                settings.resolve,
                p::PostResolve::Smaa(p::SmaaSettings {
                    threshold: 0.125,
                    max_search_steps: 112,
                    max_search_steps_diag: diagonal,
                    corner_rounding: corners,
                    edge_detection: p::SmaaEdgeDetection::Color,
                })
            );
        }
    }
}

#[test]
fn smaa_rejects_duplicate_resolves_even_with_zero_fxaa_weight() {
    let smaa = s::SceneAntiAlias::smaa(s::SceneSmaaPreset::Ultra);
    for strength in [0, 500_000] {
        assert_eq!(
            post::settings(&effects(smaa, Some(strength))),
            Err(p::PostError::UnsupportedProfile("duplicate resolve"))
        );
    }
    let authored = effects(smaa, None);
    for resolve in [
        p::PostResolve::fxaa(0.5),
        p::PostResolve::taa(0.9),
        p::PostResolve::Smaa(p::SmaaSettings::LOW),
    ] {
        assert_eq!(
            post::settings_with_antialiasing(
                Some(&authored),
                p::AntiAliasing::new(p::MsaaSamples::X1, resolve)
            ),
            Err(p::PostError::UnsupportedProfile("duplicate resolve"))
        );
    }
}

#[test]
fn existing_scene_choices_keep_their_settings_and_jitter() {
    for (authored, resolve, jitter) in [
        (s::SceneAntiAlias::Disabled, p::PostResolve::None, None),
        (
            s::SceneAntiAlias::Fxaa {
                subpixel_micro: 750_000,
            },
            p::PostResolve::fxaa(0.75),
            None,
        ),
        (
            s::SceneAntiAlias::Fxaa { subpixel_micro: 0 },
            p::PostResolve::None,
            None,
        ),
        (
            s::SceneAntiAlias::taa(900_000),
            p::PostResolve::taa(0.9),
            Some(p::ProjectionJitter::default()),
        ),
    ] {
        let settings = post::settings(&effects(authored, None)).unwrap();
        assert_eq!(
            settings,
            p::HdrPostSettings {
                resolve,
                ..Default::default()
            }
        );
        assert_eq!(
            post::jitter(
                &settings,
                p::FrameConfig::new([64, 48], wgpu::TextureFormat::Rgba16Float, 1),
                None
            ),
            Ok(jitter)
        );
    }
}
