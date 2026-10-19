use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

#[test]
fn required_gpu_concurrent_poller_readback_and_reuse() {
    let context = required_context();
    let mut target = OffscreenTarget::new(context.device(), viewport(65, 3)).unwrap();
    let stop = AtomicBool::new(false);
    let (started, started_recv) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let poller = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut polls = 0;
            while !stop.load(Ordering::Acquire) && Instant::now() < deadline {
                context.device().poll(wgpu::PollType::Poll).unwrap();
                polls += 1;
                if polls == 1 {
                    started.send(()).unwrap();
                }
                std::thread::yield_now();
            }
            polls
        });
        started_recv.recv_timeout(Duration::from_secs(5)).unwrap();
        let result = read_changed_frames(&context, &mut target);
        stop.store(true, Ordering::Release);
        assert!(
            poller.join().unwrap() > 0,
            "poller must run during readbacks"
        );
        result.unwrap();
    });
    // The same staging buffer must also remain reusable after the poller stops.
    read_changed_frames(&context, &mut target).unwrap();
}

fn read_changed_frames(
    context: &GpuContext,
    target: &mut OffscreenTarget,
) -> Result<(), OffscreenError> {
    for frame in 0..32 {
        let (color, expected) = if frame % 2 == 0 {
            (wgpu::Color::RED, [255, 0, 0, 255])
        } else {
            (wgpu::Color::GREEN, [0, 255, 0, 255])
        };
        let mut encoder = context
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color_view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        context.queue().submit([encoder.finish()]);
        let image = target.read_rgba8(context)?;
        assert_eq!(image.viewport, viewport(65, 3));
        assert_eq!(image.pixels.len(), 65 * 3 * 4);
        assert!(image.pixels.chunks_exact(4).all(|pixel| pixel == expected));
        assert_eq!(target.read_rgba8(context)?, image, "frame {frame} reuse");
    }
    Ok(())
}
