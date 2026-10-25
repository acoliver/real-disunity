use super::*;
use crate::{FrameProfile, ToneMap};
use worldrunner_render_scene::offscreen::OffscreenTarget;

#[test]
fn viewer_hdr_profiles_encode_plain_and_shadowed_terrain_and_resize() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let source = super::retained_tests::terrain_source();
    for samples in [1, 4] {
        for shadows in [None, Some(super::retained_tests::shadow_settings())] {
            let profile = FrameProfile::Hdr {
                samples,
                tone_map: ToneMap::Reinhard,
            };
            let mut resident = Resident::new(&context, &source, shadows, profile).unwrap();
            let mut presentation = Presentation::with_profile(
                &context,
                crate::presentation::COLOR,
                Viewport {
                    width: 64,
                    height: 64,
                },
                profile,
                source.effects(),
            )
            .unwrap();
            for size in [
                Viewport {
                    width: 64,
                    height: 64,
                },
                Viewport {
                    width: 37,
                    height: 29,
                },
            ] {
                presentation.resize(&context, size).unwrap();
                let options = crate::tests::options();
                let view = crate::OrbitCamera::new(options.pose, options.orbit)
                    .unwrap()
                    .view(options.camera, size)
                    .unwrap();
                let mut output = OffscreenTarget::new(context.device(), size).unwrap();
                let mut encoder = hdr_encoder(&context);
                resident
                    .render(
                        &context,
                        &view,
                        &presentation.targets,
                        &mut encoder,
                        presentation.scheduler.encoding(),
                    )
                    .unwrap();
                presentation
                    .present(&mut encoder, output.color_view())
                    .unwrap();
                output.encode_readback(&mut encoder).unwrap();
                let submission = encoder.finish().submit().unwrap();
                let image = output.read_encoded(&context, submission).unwrap();
                assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[0] > 100));
                assert_eq!(presentation.targets.depth.texture().sample_count(), samples);
            }
        }
    }
}

fn hdr_encoder(context: &GpuContext) -> worldrunner_render_gpu::submission::RecordingFrame {
    context.submission().begin(context.device()).unwrap()
}

#[test]
fn viewer_reduced_resolution_reconfigures_scene_and_output_together() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = super::retained_tests::terrain_source();
    let scale = crate::RenderScale::new(770_000).unwrap();
    let profile = FrameProfile::HdrUpscaled {
        samples: 4,
        tone_map: ToneMap::Reinhard,
        scale,
        upscale: crate::UpscaleSettings {
            mode: crate::UpscaleMode::Easu,
            sharpness: Some(0.0),
        },
    };
    let mut resident = Resident::new(&context, &source, None, profile).unwrap();
    let mut presentation = Presentation::with_profile(
        &context,
        crate::presentation::COLOR,
        Viewport {
            width: 64,
            height: 64,
        },
        profile,
        source.effects(),
    )
    .unwrap();
    for [width, height] in [[64, 64], [50, 26]] {
        let size = Viewport { width, height };
        presentation.resize(&context, size).unwrap();
        let expected = scale.extent([width, height]).unwrap();
        assert_eq!(presentation.scheduler.targets().config().size, expected);
        assert_eq!(
            presentation.scheduler.targets().config().output_extent(),
            [width, height]
        );
        let options = crate::tests::options();
        let view = crate::OrbitCamera::new(options.pose, options.orbit)
            .unwrap()
            .view(options.camera, size)
            .unwrap();
        let mut output = OffscreenTarget::new(context.device(), size).unwrap();
        let mut encoder = hdr_encoder(&context);
        resident
            .render(
                &context,
                &view,
                &presentation.targets,
                &mut encoder,
                presentation.scheduler.encoding(),
            )
            .unwrap();
        presentation
            .present(&mut encoder, output.color_view())
            .unwrap();
        output.encode_readback(&mut encoder).unwrap();
        let submission = encoder.finish().submit().unwrap();
        let image = output.read_encoded(&context, submission).unwrap();
        assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[0] > 100));
    }
}
