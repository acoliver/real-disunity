use super::*;

pub(super) fn assert_production_recovery(renderer: &mut Renderer) {
    let device = renderer.context.device().clone();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    let generation = renderer.execution.presenter.configuration_generation();
    for outcome in [
        wgpu::CurrentSurfaceTexture::Outdated,
        wgpu::CurrentSurfaceTexture::Lost,
    ] {
        renderer
            .execution
            .presenter
            .handle_acquired(&device, outcome)
            .unwrap();
        renderer.resize([700, 500]).unwrap();
        renderer.resize([0, 480]).unwrap();
        renderer.resize([640, 480]).unwrap();
        present(renderer);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
    }
    assert_eq!(
        renderer.execution.presenter.configuration_generation(),
        generation + 2
    );
    for _ in 0..8 {
        let frame = match renderer.execution.presenter.acquire(&device).unwrap() {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("{other:?}"),
        };
        let frame = match renderer
            .execution
            .presenter
            .handle_acquired(&device, wgpu::CurrentSurfaceTexture::Suboptimal(frame))
            .unwrap()
        {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("{other:?}"),
        };
        // Discarding usable images can exhaust the Vulkan swapchain instead of releasing them.
        renderer.window.pre_present_notify();
        renderer.context.queue().present(frame);
        present(renderer);
    }
    assert_eq!(
        renderer.execution.presenter.configuration_generation(),
        generation + 2
    );
}
