use crate::{
    AntiAliasing, CasSettings, FrameConfig, FxaaQuality, FxaaSettings, HdrPostSettings,
    MsaaSamples, PostError, PostResolve,
};

#[test]
fn msaa_samples_accept_exactly_one_two_four_and_eight() {
    for (count, expected) in [
        (1, MsaaSamples::X1),
        (2, MsaaSamples::X2),
        (4, MsaaSamples::X4),
        (8, MsaaSamples::X8),
    ] {
        assert_eq!(MsaaSamples::try_from(count), Ok(expected));
        assert_eq!(expected.count(), count);
        assert_eq!(u32::from(expected), count);
    }
    for count in [0, 3, 5, 6, 7, 9, 16, 32, u32::MAX] {
        assert_eq!(
            MsaaSamples::try_from(count),
            Err(PostError::SampleCount(count))
        );
    }
}

#[test]
fn default_descriptor_is_single_sample_without_resolve() {
    let aa = AntiAliasing::default();
    assert_eq!(aa.msaa, MsaaSamples::X1);
    assert_eq!(aa.resolve, PostResolve::None);
    assert_eq!(aa.validate(), Ok(()));
}

#[test]
fn descriptor_validates_the_resolve_it_carries() {
    let quality = FxaaQuality {
        edge_threshold: 2.0,
        ..FxaaQuality::default()
    };
    let aa = AntiAliasing::new(MsaaSamples::X4, PostResolve::fxaa_with(1.0, quality));
    assert_eq!(
        aa.validate(),
        Err(PostError::UnsupportedProfile("FXAA quality settings"))
    );
    for strength in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        assert_eq!(
            AntiAliasing::new(MsaaSamples::X1, PostResolve::fxaa(strength)).validate(),
            Err(PostError::UnsupportedProfile("FXAA settings")),
        );
    }
}

#[test]
fn frame_config_takes_its_sample_count_from_the_descriptor() {
    let aa = AntiAliasing::new(MsaaSamples::X8, PostResolve::None);
    let config = aa.frame_config([16, 9], wgpu::TextureFormat::Rgba16Float);
    assert_eq!(config.samples, 8);
    assert_eq!(config.size, [16, 9]);
    assert_eq!(
        config,
        FrameConfig::new([16, 9], wgpu::TextureFormat::Rgba16Float, 8)
    );
}

#[test]
fn resolve_selection_lives_in_hdr_settings_and_enables_the_path() {
    let off = HdrPostSettings::default();
    assert_eq!(off.resolve, PostResolve::None);
    assert!(!off.enabled());
    let on = HdrPostSettings {
        resolve: PostResolve::fxaa(0.5),
        ..off
    };
    assert!(on.enabled());
    assert_eq!(on.resolve.fxaa_strength(), Some(0.5));
    // A zero-strength request is validated and enabled but schedules no pass.
    let zero = PostResolve::fxaa(0.0);
    assert_eq!(zero.active_fxaa(), None);
    assert_eq!(
        PostResolve::Fxaa(FxaaSettings {
            strength: 0.25,
            quality: FxaaQuality::default()
        })
        .active_fxaa()
        .map(|fxaa| fxaa.strength),
        Some(0.25)
    );
}

#[test]
fn extended_sample_counts_need_device_support_before_allocation() {
    let limits = wgpu::Limits::default();
    let format = wgpu::TextureFormat::Rgba16Float;
    // Without adapter-specific format features, only the baseline count is guaranteed.
    for samples in [2, 8] {
        assert_eq!(
            FrameConfig::new([8, 8], format, samples).validate(&limits),
            Err(PostError::UnsupportedFormat(format)),
        );
    }
    assert_eq!(
        FrameConfig::new([8, 8], format, 4).validate(&limits),
        Ok(())
    );
    for samples in [2, 4, 8] {
        assert_eq!(
            FrameConfig::new([8, 8], format, samples).validate_extended(&limits),
            Ok(())
        );
    }
    assert_eq!(
        FrameConfig::new([8, 8], format, 3).validate_extended(&limits),
        Err(PostError::SampleCount(3)),
    );
}

#[test]
fn sharpening_is_off_by_default_and_independent_of_the_resolve() {
    assert_eq!(AntiAliasing::default().sharpen, None);
    assert_eq!(
        AntiAliasing::new(MsaaSamples::X4, PostResolve::fxaa(0.5)).sharpen,
        None
    );
    let on =
        AntiAliasing::new(MsaaSamples::X4, PostResolve::None).with_sharpen(CasSettings::new(0.5));
    assert_eq!(on.sharpen, Some(CasSettings::new(0.5)));
    assert_eq!(on.resolve, PostResolve::None);
    assert_eq!(on.validate(), Ok(()));
}

#[test]
fn sharpness_must_be_finite_and_within_zero_to_one() {
    for sharpness in [0.0, 0.5, 1.0] {
        assert_eq!(CasSettings::new(sharpness).validate(), Ok(()));
    }
    for sharpness in [-0.01, 1.01, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let settings = CasSettings::new(sharpness);
        let error = Err(PostError::UnsupportedProfile("CAS settings"));
        assert_eq!(settings.validate(), error);
        assert_eq!(
            AntiAliasing::default().with_sharpen(settings).validate(),
            error
        );
        assert_eq!(
            HdrPostSettings {
                sharpen: Some(settings),
                ..HdrPostSettings::default()
            }
            .validate(),
            error
        );
    }
}

#[test]
fn sharpening_alone_enables_the_hdr_path() {
    let sharpen = HdrPostSettings {
        sharpen: Some(CasSettings::default()),
        ..HdrPostSettings::default()
    };
    assert!(sharpen.enabled());
    assert!(!HdrPostSettings::default().enabled());
}

/// The tone stage writes the FXAA input as `Rgba16Float`, 8 bytes a texel for each of the 3
/// frame slots. An upscale after FXAA adds the resolve's `Rgba32Float` output, 16 bytes a texel.
#[test]
fn fxaa_allocates_a_half_float_display_intermediate() {
    use crate::{ColorGrading, UpscaleMode, UpscaleSettings, allocation::post_allocations};
    let limits = wgpu::Limits::default();
    let base = HdrPostSettings {
        grading: ColorGrading {
            linear_scale: 1.5,
            ..ColorGrading::default()
        },
        ..HdrPostSettings::default()
    };
    let same = FrameConfig::new([64, 32], wgpu::TextureFormat::Rgba16Float, 1);
    let bytes = |config, settings| {
        post_allocations(config, settings)
            .validate(&limits)
            .unwrap()
    };
    let fxaa = HdrPostSettings {
        resolve: PostResolve::fxaa(1.0),
        ..base
    };
    assert_eq!(bytes(same, fxaa) - bytes(same, base), 64 * 32 * 8 * 3);
    let reduced = same.with_render_extent([32, 16]);
    let upscale = Some(UpscaleSettings {
        mode: UpscaleMode::Bilinear,
        sharpness: None,
    });
    let upscaled = HdrPostSettings { upscale, ..base };
    let both = HdrPostSettings { upscale, ..fxaa };
    assert_eq!(
        bytes(reduced, both) - bytes(reduced, upscaled),
        32 * 16 * 8 * 3
    );
}
