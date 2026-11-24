use super::*;
use crate::test_support::upload_scene;
use crate::{compositor::FrameRenderer, offscreen::OffscreenTarget};
use worldrunner_render_gpu::submission::capture::CaptureWrites;
use worldrunner_render_post::FrameError;

#[test]
fn layered_production_capture_bakes_cold_and_invalidated_pages() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let pipelines = ScenePipelines::new(device, COLOR_FORMAT, 1).unwrap();
    let scene = layered_scene(TerrainSplatFilter::Bilinear);
    let mut gpu = upload_scene(device, &authority, &scene, &pipelines).unwrap();
    gpu.set_frame(context.queue(), matching_frame());
    gpu.enable_terrain_pages(device, &authority, page_source())
        .unwrap();
    let mut renderer = FrameRenderer::new(device);
    let target = OffscreenTarget::new(
        device,
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    for invalidated in [false, true] {
        if invalidated {
            gpu.invalidate_terrain_pages([0.0; 2], [1.0; 2]);
        }
        let request = gpu
            .terrain_residency_request(&matching_frame(), [TARGET; 2])
            .unwrap()
            .unwrap();
        assert!(!gpu.terrain_residency_ready(&request).unwrap());
        capture(
            &context,
            &authority,
            (&mut gpu, &pipelines),
            &mut renderer,
            &target,
        );
        assert!(gpu.terrain_residency_ready(&request).unwrap());
    }
}

fn capture(
    context: &GpuContext,
    authority: &SubmissionAuthority,
    (gpu, pipelines): (&mut SceneGpu, &ScenePipelines),
    renderer: &mut FrameRenderer,
    target: &OffscreenTarget,
) {
    let frame = gpu.resident_frame();
    let device = context.device();
    let color = target.color_attachment();
    let depth = target.depth_attachment();
    let mut keys = gpu.capture_write_keys(device).unwrap();
    let stable = gpu.capture_write_keys(device).unwrap();
    assert_eq!(keys, stable);
    keys.extend(
        renderer
            .capture_texture_keys(&[color.texture(), depth.texture()])
            .unwrap(),
    );
    keys.extend(renderer.capture_write_keys());
    let writes = CaptureWrites::new(keys.clone(), keys.clone(), keys).unwrap();
    let prefix = authority
        .begin_capture(device, writes, (), |encoder, _, ()| {
            renderer.encoding().encode_scene(
                device,
                encoder,
                (&mut *gpu, pipelines),
                crate::compositor::ColorDepth {
                    color: &color,
                    depth: &depth,
                },
                frame,
            )
        })
        .unwrap();
    let continuation = prefix.submit().unwrap();
    let suffix = continuation
        .resume(
            device,
            |_, _| Ok::<_, FrameError>(()),
            |encoder, _, (), ()| {
                renderer.encoding().encode_scene(
                    device,
                    encoder,
                    (&mut *gpu, pipelines),
                    crate::compositor::ColorDepth {
                        color: &color,
                        depth: &depth,
                    },
                    frame,
                )
            },
        )
        .unwrap();
    suffix
        .submit()
        .unwrap()
        .finish(device, |_, _| Ok::<_, FrameError>(()))
        .unwrap();
}
