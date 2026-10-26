use super::*;

pub(super) fn assert_production_recovery(renderer: &mut Renderer) {
    let device = renderer.context.device().clone();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    let generation = renderer
        .execution
        .surface()
        .presenter
        .configuration_generation();
    assert_lost_and_outdated_recovery(renderer, &device);
    assert_eq!(
        renderer
            .execution
            .surface()
            .presenter
            .configuration_generation(),
        generation + 2
    );
    assert_suboptimal_presentation(renderer, &device);
    assert_eq!(
        renderer
            .execution
            .surface()
            .presenter
            .configuration_generation(),
        generation + 2
    );
}

fn assert_lost_and_outdated_recovery(renderer: &mut Renderer, device: &wgpu::Device) {
    for outcome in [
        wgpu::CurrentSurfaceTexture::Outdated,
        wgpu::CurrentSurfaceTexture::Lost,
    ] {
        renderer
            .execution
            .surface_mut()
            .presenter
            .handle_acquired(device, outcome)
            .unwrap();
        renderer.resize([700, 500]).unwrap();
        let previous_view = view(renderer);
        let before_suspend = renderer
            .execution
            .surface()
            .presenter
            .configuration_generation();
        renderer.resize([0, 480]).unwrap();
        let mut update: crate::SceneUpdate = Box::new(|_| panic!("suspended recovery update"));
        for _ in 0..3 {
            assert!(!renderer.render(&previous_view, &mut update).unwrap());
            renderer.poll().unwrap();
        }
        assert_eq!(
            renderer
                .execution
                .surface()
                .presenter
                .configuration_generation(),
            before_suspend
        );
        renderer.resize([640, 480]).unwrap();
        present(renderer);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
    }
}

fn assert_suboptimal_presentation(renderer: &mut Renderer, device: &wgpu::Device) {
    for _ in 0..8 {
        let frame = match renderer
            .execution
            .surface_mut()
            .presenter
            .acquire(device)
            .unwrap()
        {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("{other:?}"),
        };
        let frame = match renderer
            .execution
            .surface_mut()
            .presenter
            .handle_acquired(device, wgpu::CurrentSurfaceTexture::Suboptimal(frame))
            .unwrap()
        {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("{other:?}"),
        };
        // Discarding usable images can exhaust the Vulkan swapchain instead of releasing them.
        renderer.window.as_ref().unwrap().pre_present_notify();
        renderer.context.queue().present(frame);
        present(renderer);
    }
}
