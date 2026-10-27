use super::*;
use crate::{
    CaptureJitter, CaptureOutput, RetainedCaptureOutcome, RetainedCaptureRecipe, RetainedEvent,
};
use worldrunner_render_scene::{
    compositor::FeatureTarget,
    offscreen::{LinearRgbaImage, ScalarImage},
};

fn setup(samples: u32, half_resolution: bool) -> (RetainedHost, Renderer) {
    let host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [64, 64],
            frame_profile: crate::FrameProfile::Hdr {
                samples,
                tone_map: crate::ToneMap::Reinhard,
            },
            ssr: Some(worldrunner_render_plan::SceneSsr {
                enabled: true,
                resolution_scale_micro: if half_resolution { 500_000 } else { 1_000_000 },
                ..Default::default()
            }),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    (host, renderer)
}
fn recipe() -> RetainedCaptureRecipe {
    RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![
            CaptureOutput::Srgb8,
            CaptureOutput::LinearHdr,
            CaptureOutput::Depth,
            CaptureOutput::Normals,
            CaptureOutput::AmbientOcclusion,
            CaptureOutput::SsrConfidence,
        ],
        16 * 1024 * 1024,
    )
    .unwrap()
}
fn direct(
    renderer: &Renderer,
    feature: FeatureTarget,
) -> (
    worldrunner_render_scene::Viewport,
    wgpu::TextureFormat,
    Vec<u8>,
) {
    let texture = renderer
        .presentation
        .scheduler
        .feature_target(feature)
        .unwrap();
    let viewport = worldrunner_render_scene::Viewport {
        width: texture.width(),
        height: texture.height(),
    };
    let format = texture.format();
    let mut readback = worldrunner_render_post::Readback::new(
        renderer.context.device(),
        worldrunner_render_post::FrameConfig::new([viewport.width, viewport.height], format, 1),
        1024 * 1024,
    )
    .unwrap();
    let bytes = readback
        .read(
            renderer.context.device(),
            renderer.context.submission(),
            texture,
        )
        .unwrap();
    (viewport, format, bytes)
}
#[test]
fn feature_readbacks_equal_existing_targets_at_native_extents() {
    for (samples, half) in [(1, false), (4, true)] {
        let (mut host, mut renderer) = setup(samples, half);
        let id = host.request_capture_recipe(0, recipe()).unwrap();
        assert!(renderer.render_retained(&mut host).unwrap());
        renderer.drain().unwrap();
        renderer.collect_retained(&mut host).unwrap();
        let Some(RetainedEvent::Capture {
            id: actual,
            outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
            ..
        }) = host.captures.take_event()
        else {
            panic!("recipe result");
        };
        assert_eq!(actual, id);
        assert!(products.srgb8.is_some());
        assert!(products.linear_hdr.is_some());
        for (feature, image) in [
            (FeatureTarget::Depth, products.depth.unwrap()),
            (
                FeatureTarget::AmbientOcclusion,
                products.ambient_occlusion.unwrap(),
            ),
            (
                FeatureTarget::SsrConfidence,
                products.ssr_confidence.unwrap(),
            ),
        ] {
            let (viewport, format, bytes) = direct(&renderer, feature);
            assert_eq!(
                image,
                ScalarImage::from_native(viewport, format, &bytes).unwrap()
            );
        }
        let (viewport, format, bytes) = direct(&renderer, FeatureTarget::Normals);
        assert_eq!(
            products.normals.unwrap(),
            LinearRgbaImage::from_native(viewport, format, &bytes).unwrap()
        );
    }
}
#[test]
fn submitted_feature_cancellation_delivers_once_and_releases_maps() {
    let (mut host, mut renderer) = setup(1, false);
    let id = host.request_capture_recipe(0, recipe()).unwrap();
    renderer.render_retained(&mut host).unwrap();
    host.captures.cancel();
    assert!(
        matches!(host.captures.take_event(), Some(RetainedEvent::Capture { id: actual, outcome: RetainedCaptureOutcome::Cancelled, .. }) if actual == id)
    );
    renderer.drain().unwrap();
    renderer.collect_retained(&mut host).unwrap();
    assert!(host.captures.take_event().is_none());
    assert!(!renderer.streaming.captures.is_pending());
}

fn write_confidence_lanes(
    renderer: &Renderer,
    texture: &wgpu::Texture,
    samples: u32,
    lane_width: u32,
    height: u32,
) {
    for (sample, alpha) in [0x3000_u16, 0x3400, 0x3800, 0x3c00]
        .into_iter()
        .take(usize::try_from(samples).unwrap())
        .enumerate()
    {
        let [lo, hi] = alpha.to_le_bytes();
        let bytes =
            [0, 0, 0, 0, 0, 0, lo, hi].repeat(usize::try_from(lane_width * height).unwrap());
        renderer.context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: u32::try_from(sample).unwrap() * lane_width,
                    y: 0,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(lane_width * 8),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width: lane_width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[test]
fn ssr_feature_readbacks_preserve_contiguous_sample_lanes() {
    for (samples, half, expected) in [
        (1, false, [64, 64]),
        (4, false, [256, 64]),
        (1, true, [32, 32]),
        (4, true, [128, 32]),
    ] {
        let (_, renderer) = setup(samples, half);
        let scheduler = &renderer.presentation.scheduler;
        let texture = scheduler
            .feature_target(FeatureTarget::SsrConfidence)
            .unwrap();
        assert_eq!([texture.width(), texture.height()], expected);
        let lane_width = if half { 32 } else { 64 };
        write_confidence_lanes(&renderer, texture, samples, lane_width, expected[1]);
        let device = renderer.context.device();
        let scene = scheduler.targets().texture();
        let outputs = &[CaptureOutput::SsrConfidence];
        let mut readback = Products::new(device, scene, scene, outputs, None).unwrap();
        readback.features(device, scheduler, outputs).unwrap();
        let mut encoder = renderer.context.submission().begin(device).unwrap();
        readback.encode(&mut encoder, scene, scene).unwrap();
        encoder.finish().submit().unwrap();
        readback.begin().unwrap();
        renderer.drain().unwrap();
        let image = readback
            .try_read()
            .unwrap()
            .unwrap()
            .ssr_confidence
            .unwrap();
        assert_eq!([image.viewport.width, image.viewport.height], expected);
        let returned_width = image.viewport.width;
        let trace_width = returned_width / samples;
        assert_eq!(trace_width, lane_width);
        for (sample, expected_alpha) in [0.125_f32, 0.25, 0.5, 1.0]
            .into_iter()
            .take(usize::try_from(samples).unwrap())
            .enumerate()
        {
            for y in 0..image.viewport.height {
                for x in 0..trace_width {
                    let packed_x = u32::try_from(sample).unwrap() * trace_width + x;
                    let index = usize::try_from(y * returned_width + packed_x).unwrap();
                    assert_eq!(image.pixels[index].to_bits(), expected_alpha.to_bits());
                }
            }
        }
    }
}
