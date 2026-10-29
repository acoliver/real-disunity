use crate::{
    AntiAliasing, Cmaa2Preset, Cmaa2Settings, HdrPostSettings, MsaaSamples, PostError, PostResolve,
    SmaaPreset,
};

#[test]
fn presets_match_the_reference_definitions() {
    // CMAA2.hlsl: g_CMAA2_EdgeThreshold for CMAA2_STATIC_QUALITY_PRESET 0..3.
    let expected = [
        (Cmaa2Preset::Low, 0.15),
        (Cmaa2Preset::Medium, 0.10),
        (Cmaa2Preset::High, 0.07),
        (Cmaa2Preset::Ultra, 0.05),
    ];
    for (preset, threshold) in expected {
        let cmaa2 = Cmaa2Settings::preset(preset);
        assert_eq!(cmaa2.edge_threshold.to_bits(), f32::to_bits(threshold));
        assert!(!cmaa2.extra_sharpness);
        assert_eq!(cmaa2.validate(), Ok(()));
        assert_eq!(Cmaa2Settings::from(preset), cmaa2);
        assert_eq!(PostResolve::cmaa2(preset), PostResolve::Cmaa2(cmaa2));
    }
    assert_eq!(Cmaa2Settings::default(), Cmaa2Settings::HIGH);
    assert!(Cmaa2Settings::HIGH.with_extra_sharpness().extra_sharpness);
}

#[test]
fn thresholds_outside_zero_to_half_are_rejected() {
    for threshold in [
        0.0,
        -0.1,
        0.51,
        1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let cmaa2 = Cmaa2Settings {
            edge_threshold: threshold,
            ..Cmaa2Settings::HIGH
        };
        assert_eq!(
            cmaa2.validate(),
            Err(PostError::UnsupportedProfile("CMAA2 settings")),
            "{threshold}"
        );
        assert!(PostResolve::Cmaa2(cmaa2).validate().is_err());
        assert!(
            AntiAliasing::new(MsaaSamples::X1, PostResolve::Cmaa2(cmaa2))
                .validate()
                .is_err()
        );
    }
    for threshold in [f32::MIN_POSITIVE, 0.01, 0.5] {
        let cmaa2 = Cmaa2Settings {
            edge_threshold: threshold,
            ..Cmaa2Settings::HIGH
        };
        assert_eq!(cmaa2.validate(), Ok(()), "{threshold}");
    }
}

#[test]
fn cmaa2_is_a_resolve_that_enables_the_display_path_and_excludes_the_others() {
    let settings = HdrPostSettings {
        resolve: PostResolve::cmaa2(Cmaa2Preset::Medium),
        ..HdrPostSettings::default()
    };
    assert!(settings.enabled());
    assert_eq!(settings.validate(), Ok(()));
    // One resolve is selected, so CMAA2 carries no FXAA or SMAA settings and vice versa.
    let resolve = settings.resolve;
    assert_eq!(resolve.fxaa_strength(), None);
    assert!(resolve.active_fxaa().is_none());
    assert!(resolve.active_smaa().is_none());
    assert!(resolve.active_cmaa2().is_some());
    assert!(PostResolve::fxaa(1.0).active_cmaa2().is_none());
    assert!(PostResolve::smaa(SmaaPreset::High).active_cmaa2().is_none());
    assert!(PostResolve::None.active_cmaa2().is_none());
}

#[test]
fn the_display_intermediate_is_half_precision_and_storage_capable_for_cmaa2() {
    assert_eq!(
        PostResolve::cmaa2(Cmaa2Preset::High).display_format(),
        wgpu::TextureFormat::Rgba16Float
    );
    assert_eq!(
        PostResolve::fxaa(1.0).display_format(),
        wgpu::TextureFormat::Rgba32Float
    );
}

#[test]
fn constants_carry_the_threshold_and_the_sharpness_option() {
    let constant = |cmaa2: Cmaa2Settings, name: &str| -> u64 {
        cmaa2
            .constants()
            .into_iter()
            .find(|(key, _)| *key == name)
            .unwrap()
            .1
            .to_bits()
    };
    assert_eq!(
        constant(Cmaa2Settings::ULTRA, "THRESHOLD"),
        f64::to_bits(f64::from(0.05_f32))
    );
    assert_eq!(
        constant(Cmaa2Settings::ULTRA, "EXTRA_SHARPNESS"),
        f64::to_bits(0.0)
    );
    assert_eq!(
        constant(Cmaa2Settings::LOW.with_extra_sharpness(), "EXTRA_SHARPNESS"),
        f64::to_bits(1.0)
    );
}

#[test]
fn allocation_accounting_covers_the_working_image_and_work_lists() {
    use crate::{FrameConfig, allocation::post_allocations};
    let config = FrameConfig::new([256, 128], wgpu::TextureFormat::Rgba16Float, 1);
    let limits = wgpu::Limits::default();
    let bytes = |resolve| {
        post_allocations(
            config,
            HdrPostSettings {
                resolve,
                ..HdrPostSettings::default()
            },
        )
        .validate(&limits)
        .unwrap()
    };
    let none = bytes(PostResolve::None);
    let cmaa2 = bytes(PostResolve::cmaa2(Cmaa2Preset::High));
    // Two Rgba16Float images (display input and working copy) plus the lists, for 3 frame slots.
    let images = 2 * 8 * 256 * 128 * 3;
    assert!(
        cmaa2 - none > images,
        "CMAA2 adds {} bytes, expected more than the images' {images}",
        cmaa2 - none
    );
}
