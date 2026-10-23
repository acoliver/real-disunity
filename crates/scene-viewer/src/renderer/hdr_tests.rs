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
                presentation.resize(context.device(), size);
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
                        &mut presentation.scheduler,
                    )
                    .unwrap();
                presentation
                    .present(&mut encoder, output.color_view())
                    .unwrap();
                output.encode_readback(&mut encoder).unwrap();
                let submission = context.queue().submit([encoder.finish()]);
                let image = output.read_encoded(&context, submission).unwrap();
                assert!(image.pixels.chunks_exact(4).any(|pixel| pixel[0] > 100));
                assert_eq!(presentation.targets.depth.texture().sample_count(), samples);
            }
        }
    }
}

fn hdr_encoder(context: &GpuContext) -> wgpu::CommandEncoder {
    context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
}
