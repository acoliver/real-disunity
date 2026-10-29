use crate::{
    AntiAliasing, FxaaSettings, HdrPostSettings, MsaaSamples, PostError, PostResolve,
    ProjectionJitter, SmaaPreset, TaaSettings,
};

const REJECTED: Result<(), PostError> = Err(PostError::UnsupportedProfile("TAA settings"));

#[test]
fn defaults_are_a_ninety_percent_history_and_a_unit_clip() {
    assert_eq!(
        TaaSettings::DEFAULT,
        TaaSettings {
            history_weight: 0.9,
            clip_scale: 1.0,
        }
    );
    assert_eq!(TaaSettings::default(), TaaSettings::DEFAULT);
    assert_eq!(
        TaaSettings::new(0.5),
        TaaSettings {
            history_weight: 0.5,
            ..TaaSettings::DEFAULT
        }
    );
    assert_eq!(TaaSettings::DEFAULT.validate(), Ok(()));
}

#[test]
fn history_weight_is_finite_and_within_zero_to_ninety_nine_percent() {
    for weight in [0.0, 0.5, 0.9, 0.99] {
        assert_eq!(TaaSettings::new(weight).validate(), Ok(()));
    }
    for weight in [
        -0.01,
        0.991,
        1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert_eq!(TaaSettings::new(weight).validate(), REJECTED);
    }
}

#[test]
fn clip_scale_is_finite_and_within_half_to_two() {
    for clip_scale in [0.5, 1.0, 2.0] {
        let settings = TaaSettings {
            clip_scale,
            ..TaaSettings::DEFAULT
        };
        assert_eq!(settings.validate(), Ok(()));
    }
    for clip_scale in [0.49, 2.01, 0.0, -1.0, f32::NAN, f32::INFINITY] {
        let settings = TaaSettings {
            clip_scale,
            ..TaaSettings::DEFAULT
        };
        assert_eq!(settings.validate(), REJECTED);
    }
}

#[test]
fn constructors_build_the_taa_resolve() {
    assert_eq!(
        PostResolve::taa(0.75),
        PostResolve::Taa(TaaSettings::new(0.75))
    );
    let settings = TaaSettings {
        history_weight: 0.8,
        clip_scale: 1.5,
    };
    assert_eq!(PostResolve::taa_with(settings), PostResolve::Taa(settings));
}

#[test]
fn resolve_validation_delegates_to_the_settings() {
    assert_eq!(PostResolve::taa(0.9).validate(), Ok(()));
    assert_eq!(PostResolve::taa(1.0).validate(), REJECTED);
    assert_eq!(
        AntiAliasing::new(MsaaSamples::X1, PostResolve::taa(f32::NAN)).validate(),
        REJECTED
    );
    assert_eq!(
        HdrPostSettings {
            resolve: PostResolve::taa(-0.5),
            ..HdrPostSettings::default()
        }
        .validate(),
        REJECTED
    );
}

#[test]
fn taa_enables_the_hdr_path_but_needs_no_display_referred_intermediate() {
    let resolve = PostResolve::taa(0.9);
    assert!(resolve.requested());
    assert!(!resolve.schedules_pass());
    assert_eq!(
        resolve.display_format(),
        PostResolve::None.display_format(),
        "TAA runs before tone mapping and leaves the intermediate format at its default"
    );
    let settings = HdrPostSettings {
        resolve,
        ..HdrPostSettings::default()
    };
    assert!(settings.enabled());
}

#[test]
fn active_taa_is_only_present_for_the_taa_resolve() {
    assert_eq!(
        PostResolve::taa(0.9).active_taa(),
        Some(TaaSettings::new(0.9))
    );
    assert_eq!(PostResolve::None.active_taa(), None);
    assert_eq!(PostResolve::fxaa(0.5).active_taa(), None);
    assert_eq!(PostResolve::smaa(SmaaPreset::High).active_taa(), None);
}

#[test]
fn taa_without_explicit_jitter_gets_the_default_sequence() {
    let aa = AntiAliasing::new(MsaaSamples::X1, PostResolve::taa(0.9));
    assert_eq!(aa.jitter, None);
    assert_eq!(aa.effective_jitter(), Some(ProjectionJitter::default()));
    assert_eq!(
        aa.resolve.effective_jitter(None),
        Some(ProjectionJitter::default())
    );
}

#[test]
fn explicit_jitter_wins_over_the_taa_default() {
    let jitter = ProjectionJitter::new(16).unwrap();
    let aa = AntiAliasing::new(MsaaSamples::X4, PostResolve::taa(0.9)).with_jitter(jitter);
    assert_eq!(aa.effective_jitter(), Some(jitter));
}

#[test]
fn other_resolves_keep_their_explicit_jitter_choice() {
    let jitter = ProjectionJitter::new(4).unwrap();
    for resolve in [
        PostResolve::None,
        PostResolve::fxaa(0.5),
        PostResolve::smaa(SmaaPreset::High),
        PostResolve::Fxaa(FxaaSettings {
            strength: 0.0,
            quality: crate::FxaaQuality::default(),
        }),
    ] {
        let plain = AntiAliasing::new(MsaaSamples::X1, resolve);
        assert_eq!(plain.effective_jitter(), None, "{resolve:?}");
        assert_eq!(plain.with_jitter(jitter).effective_jitter(), Some(jitter));
    }
}

#[test]
fn the_uniform_block_is_eighty_bytes_with_a_column_major_matrix_then_jitter_and_validity() {
    use crate::{TAA_UNIFORM_BYTES, TaaUniform};
    let mut reprojection = [[0.0; 4]; 4];
    for (c, column) in reprojection.iter_mut().enumerate() {
        for (r, v) in column.iter_mut().enumerate() {
            *v = f64::from(u8::try_from(c * 4 + r).unwrap());
        }
    }
    let uniform = TaaUniform {
        reprojection,
        jitter_pixels: [0.25, -0.5],
        history_valid: true,
    };
    let bytes = uniform.to_bytes();
    assert_eq!(TAA_UNIFORM_BYTES, 80);
    assert_eq!(bytes.len(), 80);
    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let expected: Vec<f32> = (0..16u8).map(f32::from).collect();
    assert_eq!(floats[..16], expected[..]);
    assert_eq!(floats[16..], [0.25, -0.5, 1.0, 0.0]);
    let none = TaaUniform {
        history_valid: false,
        ..uniform
    };
    assert_eq!(none.to_bytes()[72..76], [0; 4]);
}
