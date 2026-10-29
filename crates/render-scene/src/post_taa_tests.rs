//! Authored and descriptor TAA choices reach the post settings and the jitter rule.
use crate::post;
use worldrunner_render_plan as s;
use worldrunner_render_post as p;

pub(crate) fn effects(anti_alias: s::SceneAntiAlias, fxaa: Option<u32>) -> s::SceneEffects {
    let effects = s::SceneEffects::new(s::SceneEffectsParts {
        water: None,
        sky: s::SceneSky::ProceduralHemisphere {
            zenith: s::SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: s::SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: s::SceneSun {
            direction: s::SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 1_000_000,
        },
        ambient: s::SceneAmbient {
            sky_irradiance_micro: 0,
            ground_irradiance_micro: 0,
        },
        shadows: s::SceneShadowCascades::new(vec![4_000], 0).unwrap(),
        ssao: None,
        camera: s::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 20_000,
        },
        grading: s::SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias,
    })
    .unwrap();
    match fxaa {
        None => effects,
        Some(subpixel) => effects
            .with_hdr_post(s::SceneHdrPost {
                tone_map: None,
                grading: None,
                adaptive_exposure: None,
                bloom: None,
                fxaa_subpixel_micro: Some(subpixel),
                fxaa_quality: None,
            })
            .unwrap(),
    }
}

fn taa(history_micro: u32) -> s::SceneAntiAlias {
    s::SceneAntiAlias::Taa { history_micro }
}

const DUPLICATE: Result<p::HdrPostSettings, p::PostError> =
    Err(p::PostError::UnsupportedProfile("duplicate resolve"));

#[test]
fn authored_taa_history_becomes_the_history_weight_with_the_default_clip() {
    for (micro, weight) in [(0, 0.0), (500_000, 0.5), (900_000, 0.9), (990_000, 0.99)] {
        let settings = post::settings(&effects(taa(micro), None)).unwrap();
        assert_eq!(settings.resolve, p::PostResolve::taa(weight), "{micro}");
        assert_eq!(
            settings.resolve.active_taa().map(|taa| taa.clip_scale),
            Some(p::TaaSettings::DEFAULT.clip_scale)
        );
    }
}

#[test]
fn a_history_weight_the_settings_reject_gives_the_typed_settings_error() {
    for micro in [990_001, 1_000_000] {
        assert_eq!(
            post::settings(&effects(taa(micro), None)),
            Err(p::PostError::UnsupportedProfile("TAA settings")),
            "{micro}"
        );
    }
}

#[test]
fn authored_taa_and_authored_fxaa_are_one_resolve_too_many() {
    assert_eq!(
        post::settings(&effects(taa(900_000), Some(500_000))),
        DUPLICATE
    );
}

#[test]
fn a_descriptor_resolve_and_an_authored_resolve_conflict_in_either_order() {
    let authored_taa = effects(taa(900_000), None);
    let authored_fxaa = effects(s::SceneAntiAlias::Disabled, Some(500_000));
    let aa = |resolve| p::AntiAliasing::new(p::MsaaSamples::X1, resolve);
    for (effects, resolve) in [
        (&authored_taa, p::PostResolve::taa(0.5)),
        (&authored_taa, p::PostResolve::fxaa(0.5)),
        (&authored_fxaa, p::PostResolve::taa(0.5)),
    ] {
        assert_eq!(
            post::settings_with_antialiasing(Some(effects), aa(resolve)),
            DUPLICATE
        );
    }
}

#[test]
fn a_descriptor_taa_or_an_authored_taa_alone_selects_the_resolve() {
    let aa = |resolve| p::AntiAliasing::new(p::MsaaSamples::X1, resolve);
    let descriptor = post::settings_with_antialiasing(None, aa(p::PostResolve::taa(0.8))).unwrap();
    assert_eq!(descriptor.resolve, p::PostResolve::taa(0.8));
    let authored = post::settings_with_antialiasing(
        Some(&effects(taa(800_000), None)),
        aa(p::PostResolve::None),
    )
    .unwrap();
    assert_eq!(authored.resolve, p::PostResolve::taa(0.8));
}

#[test]
fn the_merged_resolve_decides_the_jitter_whichever_route_chose_it() {
    let explicit = p::ProjectionJitter::new(16).unwrap();
    let default = Some(p::ProjectionJitter::default());
    let from_authoring = post::settings(&effects(taa(900_000), None)).unwrap();
    let from_descriptor = post::settings_with_antialiasing(
        None,
        p::AntiAliasing::new(p::MsaaSamples::X1, p::PostResolve::taa(0.9)),
    )
    .unwrap();
    for settings in [from_authoring, from_descriptor] {
        assert_eq!(post::jitter(&settings, None), default);
        assert_eq!(post::jitter(&settings, Some(explicit)), Some(explicit));
    }
    let fxaa = post::settings(&effects(s::SceneAntiAlias::Disabled, Some(500_000))).unwrap();
    assert_eq!(post::jitter(&fxaa, None), None);
    assert_eq!(post::jitter(&fxaa, Some(explicit)), Some(explicit));
    assert_eq!(post::jitter(&p::HdrPostSettings::default(), None), None);
}

#[test]
fn taa_is_refused_until_the_resolve_exists() {
    let unavailable = Err(p::PostError::UnsupportedProfile(
        "TAA resolve not yet available",
    ));
    let taa_settings = post::settings(&effects(taa(900_000), None)).unwrap();
    assert_eq!(post::require_available(&taa_settings), unavailable);
    for resolve in [p::PostResolve::None, p::PostResolve::fxaa(0.5)] {
        let settings = p::HdrPostSettings {
            resolve,
            ..p::HdrPostSettings::default()
        };
        assert_eq!(post::require_available(&settings), Ok(()));
    }
}
