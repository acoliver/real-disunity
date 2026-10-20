use worldrunner_render_gpu::{device::*, texture::*};

#[test]
fn required_limits_and_features_are_never_lowered() {
    let available = wgpu::Limits::default();
    let requirements = DeviceRequirements {
        required_limits: wgpu::Limits {
            max_bind_groups: 8,
            ..available.clone()
        },
        ..Default::default()
    };
    assert!(matches!(
        negotiate(wgpu::Features::empty(), &available, &requirements),
        Err(DeviceError::LimitsExceeded { .. })
    ));
    let requirements = DeviceRequirements {
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..Default::default()
    };
    assert!(matches!(
        negotiate(wgpu::Features::empty(), &available, &requirements),
        Err(DeviceError::MissingFeatures { .. })
    ));
}

#[test]
fn unsupported_optional_features_are_explicit() {
    let requirements = DeviceRequirements {
        optional_features: wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    };
    let result = negotiate(
        wgpu::Features::empty(),
        &wgpu::Limits::default(),
        &requirements,
    )
    .unwrap();
    assert_eq!(result.unavailable_optional, wgpu::Features::TIMESTAMP_QUERY);
    assert_eq!(result.enabled_features, wgpu::Features::empty());
}

#[test]
fn malformed_mips_and_layers_fail_before_gpu_allocation() {
    let data = [0; 16];
    let mip = MipLevel {
        width: 2,
        height: 2,
        bytes_per_row: 8,
        rows: 2,
        bytes: &data,
    };
    let layer = TextureLayer {
        format: wgpu::TextureFormat::Rgba8Unorm,
        width: 2,
        height: 2,
        mips: &[mip],
    };
    assert!(TextureUpload::new(&[layer], 0, TextureKind::D2).is_ok());
    let short = MipLevel {
        bytes: &data[..15],
        ..mip
    };
    let bad = TextureLayer {
        mips: &[short],
        ..layer
    };
    assert!(matches!(
        TextureUpload::new(&[bad], 0, TextureKind::D2),
        Err(UploadError::Payload { .. })
    ));
    assert!(matches!(
        TextureUpload::new(
            &[layer, TextureLayer { width: 3, ..layer }],
            0,
            TextureKind::Array
        ),
        Err(UploadError::LayerMismatch { .. })
    ));
    assert!(matches!(
        TextureUpload::new(&[layer], 0, TextureKind::Cube),
        Err(UploadError::CubeLayers)
    ));
    assert!(matches!(
        TextureUpload::new(&[layer], 1, TextureKind::D2),
        Err(UploadError::BaseMip { .. })
    ));
}

#[test]
fn npot_bc_mip_layout_uses_whole_blocks_without_changing_logical_size() {
    let data = [0; 120];
    let levels = packed_mips(wgpu::TextureFormat::Bc1RgbaUnorm, 12, 20, 3, &data);
    assert!(matches!(levels, Err(UploadError::Payload { .. })));
    let data = [0; 168]; // Shorter than the 120 + 48 + 16 byte chain.
    let levels = packed_mips(wgpu::TextureFormat::Bc1RgbaUnorm, 12, 20, 3, &data);
    assert!(matches!(levels, Err(UploadError::Payload { .. })));
    let data = [0; 184]; // 120 + 48 + 16
    let levels = packed_mips(wgpu::TextureFormat::Bc1RgbaUnorm, 12, 20, 3, &data).unwrap();
    assert_eq!(
        (
            levels[1].width,
            levels[1].height,
            levels[1].bytes_per_row,
            levels[1].rows
        ),
        (6, 10, 16, 3)
    );
}

#[test]
fn optional_limit_profile_is_accepted_or_reported_without_lowering_required_limits() {
    let requirements = DeviceRequirements {
        optional_limits: Some(wgpu::Limits {
            max_bind_groups: 8,
            ..Default::default()
        }),
        ..Default::default()
    };
    let result = negotiate(
        wgpu::Features::empty(),
        &wgpu::Limits::default(),
        &requirements,
    )
    .unwrap();
    assert_eq!(result.limits.max_bind_groups, 4);
    assert!(!result.unavailable_optional_limits.is_empty());
    let available = wgpu::Limits {
        max_bind_groups: 8,
        ..Default::default()
    };
    let result = negotiate(wgpu::Features::empty(), &available, &requirements).unwrap();
    assert_eq!(result.limits.max_bind_groups, 8);
    assert!(result.unavailable_optional_limits.is_empty());
}

#[test]
fn zero_extent_and_surface_retry_outcomes_are_explicit() {
    use worldrunner_render_gpu::surface::*;
    assert!(!validate_extent([0, u32::MAX], 1).unwrap());
    assert!(matches!(
        validate_extent([2, 1], 1),
        Err(SurfaceError::Extent { .. })
    ));
    assert!(matches!(
        classify(wgpu::CurrentSurfaceTexture::Timeout).unwrap(),
        AcquiredFrame::Skip
    ));
    assert!(matches!(
        classify(wgpu::CurrentSurfaceTexture::Occluded).unwrap(),
        AcquiredFrame::Skip
    ));
    assert!(matches!(
        classify(wgpu::CurrentSurfaceTexture::Outdated).unwrap(),
        AcquiredFrame::Reconfigure
    ));
    assert!(matches!(
        classify(wgpu::CurrentSurfaceTexture::Lost).unwrap(),
        AcquiredFrame::Recreate
    ));
    assert!(matches!(
        classify(wgpu::CurrentSurfaceTexture::Validation),
        Err(SurfaceError::Validation)
    ));
}

#[test]
fn capture_unpacks_padded_bgra_without_changing_transfer_function() {
    use worldrunner_render_gpu::surface::unpack_rgba8;
    for format in [
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        assert_eq!(
            unpack_rgba8(
                format,
                [1, 2],
                8,
                &[1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8, 0, 0, 0, 0]
            )
            .unwrap(),
            [3, 2, 1, 4, 7, 6, 5, 8]
        );
    }
}

#[test]
fn automatic_present_modes_are_valid_without_being_advertised() {
    use worldrunner_render_gpu::surface::*;
    let caps = wgpu::SurfaceCapabilities {
        formats: vec![wgpu::TextureFormat::Bgra8Unorm],
        format_capabilities: vec![],
        present_modes: vec![wgpu::PresentMode::Fifo],
        alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
        usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
    };
    for present_mode in [wgpu::PresentMode::AutoVsync, wgpu::PresentMode::AutoNoVsync] {
        let config = configuration(
            &caps,
            SurfacePolicy {
                format: caps.formats[0],
                usage: caps.usages,
                size: [640, 480],
                present_mode,
                desired_maximum_frame_latency: 2,
                alpha_mode: caps.alpha_modes[0],
            },
        )
        .unwrap();
        assert_eq!(config.present_mode, present_mode);
    }
}

#[test]
fn automatic_alpha_uses_supported_opaque_or_inherit_fallbacks() {
    use worldrunner_render_gpu::surface::*;
    let mut caps = wgpu::SurfaceCapabilities {
        formats: vec![wgpu::TextureFormat::Bgra8Unorm],
        present_modes: vec![wgpu::PresentMode::Fifo],
        alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
        usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
        ..Default::default()
    };
    let policy = SurfacePolicy {
        format: caps.formats[0],
        usage: caps.usages,
        size: [640, 480],
        present_mode: wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
    };
    assert!(configuration(&caps, policy).is_ok());
    caps.alpha_modes = vec![wgpu::CompositeAlphaMode::Inherit];
    assert!(configuration(&caps, policy).is_ok());
    caps.alpha_modes = vec![wgpu::CompositeAlphaMode::PostMultiplied];
    assert!(matches!(
        configuration(&caps, policy),
        Err(SurfaceError::UnsupportedAlphaMode(
            wgpu::CompositeAlphaMode::Auto
        ))
    ));
}
