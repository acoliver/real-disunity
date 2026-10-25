use super::*;

#[test]
fn invalid_frame_contracts_are_typed() {
    let limits = wgpu::Limits::default();
    let mut config = FrameConfig::new([0, 8], wgpu::TextureFormat::Rgba16Float, 1);
    assert_eq!(config.validate(&limits), Err(PostError::EmptyExtent));
    config.size = [8, 8];
    config.samples = 3;
    assert_eq!(config.validate(&limits), Err(PostError::SampleCount(3)));
    config.samples = 1;
    config.size = [limits.max_texture_dimension_2d + 1, 8];
    assert!(matches!(
        config.validate(&limits),
        Err(PostError::TextureLimit { .. })
    ));
}

#[test]
fn depth_and_stencil_are_not_color_frame_targets() {
    for format in [
        wgpu::TextureFormat::Depth16Unorm,
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureFormat::Stencil8,
    ] {
        for samples in [1, 4] {
            assert_eq!(
                FrameConfig::new([8, 8], format, samples).validate(&wgpu::Limits::default()),
                Err(PostError::NonColorFormat(format)),
            );
        }
    }
}

#[test]
fn multisampled_frames_require_resolve_support_not_just_sample_count() {
    for format in [
        wgpu::TextureFormat::Rgba8Uint,
        wgpu::TextureFormat::Rgba8Sint,
        wgpu::TextureFormat::R32Float,
    ] {
        assert_eq!(
            FrameConfig::new([8, 8], format, 1).validate(&wgpu::Limits::default()),
            Ok(()),
            "single-sample color targets need no resolve",
        );
        assert_eq!(
            FrameConfig::new([8, 8], format, 4).validate(&wgpu::Limits::default()),
            Err(PostError::UnsupportedResolve(format)),
        );
    }
}

#[test]
fn normalized_and_hdr_frame_targets_keep_supported_resolves() {
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Rgba16Float,
    ] {
        for samples in [1, 4] {
            assert_eq!(
                FrameConfig::new([8, 8], format, samples).validate(&wgpu::Limits::default()),
                Ok(()),
            );
        }
    }
}

#[test]
fn explicit_render_extent_is_bounded_by_presentation() {
    let limits = wgpu::Limits::default();
    let native = FrameConfig::new([50, 26], wgpu::TextureFormat::Rgba16Float, 1);
    assert_eq!(native.output_extent(), native.size);
    let reduced = native.with_render_extent([33, 17]);
    assert_eq!(reduced.size, [33, 17]);
    assert_eq!(reduced.output_extent(), [50, 26]);
    assert_eq!(reduced.validate(&limits), Ok(()));
    for size in [[51, 17], [33, 27]] {
        assert_eq!(
            native.with_render_extent(size).validate(&limits),
            Err(PostError::RenderExtent)
        );
    }
    for size in [[0, 17], [33, 0]] {
        assert_eq!(
            native.with_render_extent(size).validate(&limits),
            Err(PostError::EmptyExtent)
        );
    }
    let excessive = FrameConfig::new([limits.max_texture_dimension_2d + 1, 26], native.format, 1)
        .with_render_extent([33, 17]);
    assert!(matches!(
        excessive.validate(&limits),
        Err(PostError::TextureLimit { .. })
    ));
}
