//! The temporal mask resource: when it exists, what it costs and which host textures are rejected.
use crate::hdr_post::HdrPost;
use crate::hdr_temporal_build_tests as build;
use crate::{
    FrameConfig, FrameTargets, HdrPostSettings, PostError, PostResolve, StageFactory,
    TEMPORAL_MASK_FORMAT, TargetOptions, UpscaleMode, UpscaleSettings,
};

const RENDER: [u32; 2] = [10, 6];
const OUTPUT: [u32; 2] = [16, 12];

fn config() -> FrameConfig {
    FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(RENDER)
}

fn settings(resolve: PostResolve, mode: Option<UpscaleMode>) -> HdrPostSettings {
    HdrPostSettings {
        resolve,
        upscale: mode.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..Default::default()
    }
}

#[test]
fn only_the_temporal_upscale_asks_for_masks() {
    let (device, _) = crate::gpu_tests::gpu();
    let needs = |settings| build::public_factory(&device, &settings).needs_temporal_masks();
    assert!(needs(settings(
        PostResolve::taa(0.9),
        Some(UpscaleMode::Temporal)
    )));
    for mode in [None, Some(UpscaleMode::Bilinear), Some(UpscaleMode::Easu)] {
        assert!(!needs(settings(PostResolve::taa(0.9), mode)), "{mode:?}");
    }
    assert!(!needs(settings(PostResolve::fxaa(0.5), None)));
    assert!(!needs(settings(PostResolve::None, None)));
}

#[test]
fn targets_carry_a_mask_only_when_asked_and_it_matches_the_internal_extent() {
    let (device, _) = crate::gpu_tests::gpu();
    let plain = FrameTargets::new(&device, config()).unwrap();
    assert!(plain.temporal_mask().is_none() && plain.temporal_mask_texture().is_none());
    let options = TargetOptions {
        velocity: false,
        temporal_mask: true,
    };
    let masked = FrameTargets::with_options(&device, config(), options).unwrap();
    let texture = masked.temporal_mask_texture().unwrap();
    assert!(masked.temporal_mask().is_some());
    assert_eq!(texture.format(), TEMPORAL_MASK_FORMAT);
    assert_eq!([texture.width(), texture.height()], RENDER);
    assert_eq!(texture.sample_count(), 1);
    assert_eq!(
        FrameTargets::validate_temporal_mask(config(), texture),
        Ok(())
    );
}

fn host_texture(
    device: &wgpu::Device,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    samples: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("host mask"),
        size: crate::targets::extent(size),
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

#[test]
fn a_malformed_host_mask_is_rejected_with_a_typed_error_and_no_encoder() {
    let (device, _) = crate::gpu_tests::gpu();
    let check = |size, format, samples| {
        let texture = host_texture(&device, size, format, samples);
        FrameTargets::validate_temporal_mask(config(), &texture)
    };
    let good = TEMPORAL_MASK_FORMAT;
    assert_eq!(check(RENDER, good, 1), Ok(()));
    assert_eq!(
        check(OUTPUT, good, 1),
        Err(PostError::MaskExtent {
            expected: RENDER,
            actual: OUTPUT
        })
    );
    assert_eq!(
        check([RENDER[0], RENDER[1] + 1], good, 1),
        Err(PostError::MaskExtent {
            expected: RENDER,
            actual: [RENDER[0], RENDER[1] + 1]
        })
    );
    let wrong = wgpu::TextureFormat::Rgba8Unorm;
    assert_eq!(
        check(RENDER, wrong, 1),
        Err(PostError::UnsupportedFormat(wrong))
    );
    assert_eq!(check(RENDER, good, 4), Err(PostError::SampleCount(4)));
}

fn stage_names(targets: &FrameTargets, device: &wgpu::Device) -> Vec<Vec<&'static str>> {
    let post = HdrPost::new(device, &build::settings()).unwrap();
    build::slots(&post, device, targets)
        .iter()
        .map(|set| set.stages.iter().map(|stage| stage.id.name).collect())
        .collect()
}

#[test]
fn a_frame_without_a_mask_schedules_the_stages_it_did_before_masks_existed() {
    let (device, _) = crate::gpu_tests::gpu();
    let velocity = TargetOptions {
        velocity: true,
        temporal_mask: false,
    };
    let absent = FrameTargets::with_options(&device, config(), velocity).unwrap();
    assert!(absent.temporal_mask().is_none());
    let present = build::targets(&device, RENDER, OUTPUT);
    assert!(present.temporal_mask().is_some());
    assert_eq!(
        stage_names(&absent, &device),
        stage_names(&present, &device),
        "the mask has no consumer before the preparation stage exists"
    );
}

/// The mask entry appears exactly in the configurations that ask for masks.
#[test]
fn only_temporal_upscale_with_taa_declares_mask_bytes() {
    let limits = wgpu::Limits::default();
    let masks = |config: FrameConfig, settings| {
        let set = crate::allocation::post_allocations(config, settings);
        assert!(set.validate(&limits).is_ok());
        format!("{set:?}").matches("frame temporal mask").count()
    };
    for mode in [None, Some(UpscaleMode::Bilinear), Some(UpscaleMode::Easu)] {
        for resolve in [
            PostResolve::None,
            PostResolve::fxaa(0.5),
            PostResolve::taa(0.9),
        ] {
            assert_eq!(masks(config(), settings(resolve, mode)), 0, "{mode:?}");
        }
    }
    let temporal = Some(UpscaleMode::Temporal);
    assert_eq!(
        masks(config(), settings(PostResolve::taa(0.9), temporal)),
        1
    );
    assert_eq!(masks(config(), settings(PostResolve::None, temporal)), 0);
}
