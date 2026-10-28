use crate::{
    AntiAliasing, FrameConfig, FxaaQuality, FxaaSettings, HdrPostSettings, MsaaSamples, PostError,
    PostResolve,
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
    let mut quality = FxaaQuality::default();
    quality.edge_threshold = 2.0;
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
