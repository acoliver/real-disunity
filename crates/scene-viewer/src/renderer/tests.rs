use std::time::Duration;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::{pump_events::EventLoopExtPumpEvents, x11::EventLoopBuilderExtX11},
    window::WindowId,
};

use super::*;

mod live;

#[derive(Default)]
struct Harness {
    renderer: Option<Renderer>,
}

impl ApplicationHandler for Harness {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_none() {
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(winit::dpi::PhysicalSize::new(800, 600)),
                )
                .unwrap();
            self.renderer = Some(
                pollster::block_on(Renderer::new(
                    event_loop.owned_display_handle(),
                    Arc::new(window),
                    &crate::tests::scene(false),
                    None,
                ))
                .unwrap(),
            );
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let WindowEvent::Resized(size) = event {
            self.renderer
                .as_mut()
                .unwrap()
                .resize([size.width, size.height])
                .unwrap();
        }
    }
}

#[test]
fn actual_resize_and_consecutive_suboptimal_frames_are_presented() {
    let mut event_loop = EventLoop::builder()
        .with_x11()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut harness = Harness::default();
    event_loop.pump_app_events(Some(Duration::ZERO), &mut harness);
    assert_native_resizes(&mut event_loop, &mut harness);
    let renderer = harness.renderer.as_mut().unwrap();
    let view = view(renderer);
    assert_presenter_configuration_errors(renderer);
    assert_suboptimal(renderer, &view);
    assert_recovery(renderer, &view);
    assert_resize_rejection_and_suspension(renderer, &view);
    renderer
        .context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_errors().unwrap();
    let window = renderer.window.clone();
    harness.renderer = None;
    live::pending_survives_resize_and_surface_recovery(&mut event_loop, window);
}

fn view(renderer: &Renderer) -> SceneView {
    let scene = crate::tests::scene(false);
    let options = crate::tests::options();
    crate::OrbitCamera::new(options.pose, options.orbit)
        .unwrap()
        .view(*scene.effects().camera(), renderer.viewport.unwrap())
        .unwrap()
}

fn assert_native_resizes(event_loop: &mut EventLoop<()>, harness: &mut Harness) {
    for (physical, intermediate) in [
        ([5120, 2880], [4096, 2304]),
        ([2880, 5120], [2304, 4096]),
        ([5001, 2003], [4096, 1641]),
        ([640, 480], [640, 480]),
    ] {
        let _ = harness
            .renderer
            .as_ref()
            .unwrap()
            .window
            .request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            event_loop.pump_app_events(Some(Duration::from_millis(10)), harness);
            if harness.renderer.as_ref().unwrap().viewport
                == Some(Viewport {
                    width: physical[0],
                    height: physical[1],
                })
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "resize event timed out"
            );
        }
        let renderer = harness.renderer.as_mut().unwrap();
        assert_eq!(
            [
                renderer.presenter.configuration().width,
                renderer.presenter.configuration().height
            ],
            physical
        );
        let texture = renderer.presentation.targets.color.texture();
        assert_eq!([texture.width(), texture.height()], intermediate);
        let scene = crate::tests::scene(false);
        let options = crate::tests::options();
        let camera = crate::OrbitCamera::new(options.pose, options.orbit).unwrap();
        let view = camera
            .view(*scene.effects().camera(), renderer.viewport.unwrap())
            .unwrap();
        assert!(renderer.render(&view).unwrap());
    }
}

fn assert_suboptimal(renderer: &mut Renderer, view: &SceneView) {
    let configurations = renderer.presenter.configuration_generation();
    for _ in 0..8 {
        let acquired = renderer
            .presenter
            .acquire(renderer.context.device())
            .unwrap();
        let frame = match acquired {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("expected usable actual surface frame, got {other:?}"),
        };
        assert!(
            renderer
                .render_acquired(view, wgpu::CurrentSurfaceTexture::Suboptimal(frame))
                .unwrap()
        );
    }
    assert_eq!(
        renderer.presenter.configuration_generation(),
        configurations,
        "usable frames must not cause a configure loop"
    );
}

fn assert_recovery(renderer: &mut Renderer, view: &SceneView) {
    let configurations = renderer.presenter.configuration_generation();
    assert!(
        !renderer
            .render_acquired(view, wgpu::CurrentSurfaceTexture::Timeout)
            .unwrap()
    );
    assert!(
        !renderer
            .render_acquired(view, wgpu::CurrentSurfaceTexture::Occluded)
            .unwrap()
    );
    assert_eq!(
        renderer.presenter.configuration_generation(),
        configurations
    );
    assert!(
        !renderer
            .render_acquired(view, wgpu::CurrentSurfaceTexture::Outdated)
            .unwrap()
    );
    assert_eq!(
        renderer.presenter.configuration_generation(),
        configurations + 1
    );
    assert!(
        !renderer
            .render_acquired(view, wgpu::CurrentSurfaceTexture::Lost)
            .unwrap()
    );
    assert_eq!(
        renderer.presenter.configuration_generation(),
        configurations + 2
    );
    assert!(renderer.render(view).unwrap());
    assert!(matches!(
        renderer.render_acquired(view, wgpu::CurrentSurfaceTexture::Validation),
        Err(ViewerError::SurfaceValidation)
    ));
}

fn assert_resize_rejection_and_suspension(renderer: &mut Renderer, view: &SceneView) {
    let configurations = renderer.presenter.configuration_generation();
    let previous = renderer.viewport;
    let limit = renderer.context.device().limits().max_texture_dimension_2d;
    assert!(matches!(
        renderer.resize([limit + 1, 100]),
        Err(ViewerError::SurfaceExtent { .. })
    ));
    assert_eq!(renderer.viewport, previous);
    assert_eq!(
        renderer.presenter.configuration_generation(),
        configurations
    );
    renderer.resize([0, 480]).unwrap();
    assert_eq!(renderer.viewport, None);
    assert!(!renderer.render(view).unwrap());
    assert_eq!(
        renderer.presenter.configuration_generation(),
        configurations
    );
    renderer.resize([640, 480]).unwrap();
    assert!(renderer.render(view).unwrap());
}

fn assert_presenter_configuration_errors(renderer: &mut Renderer) {
    use worldrunner_render_gpu::surface::{Presenter, SurfaceError, SurfacePolicy};
    let surface = renderer
        .instance
        .create_surface(renderer.window.clone())
        .unwrap();
    let capabilities = surface.get_capabilities(renderer.context.adapter());
    let config = renderer.presenter.configuration();
    let mut policy = SurfacePolicy {
        format: config.format,
        usage: config.usage,
        size: [0, 480],
        present_mode: wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
    };
    let mut presenter =
        Presenter::new(surface, &capabilities, renderer.context.device(), policy).unwrap();
    assert!(presenter.is_suspended());
    assert!(matches!(
        presenter.acquire(renderer.context.device()).unwrap(),
        worldrunner_render_gpu::surface::AcquiredFrame::Skip
    ));
    assert_eq!(presenter.configuration_generation(), 0);
    presenter
        .resize(renderer.context.device(), [800, 600])
        .unwrap();
    assert!(!presenter.is_suspended());
    assert_eq!(presenter.configuration_generation(), 1);
    assert_configuration_failure_is_recoverable(&mut presenter, renderer.context.device());
    presenter
        .handle_acquired(renderer.context.device(), wgpu::CurrentSurfaceTexture::Lost)
        .unwrap();
    presenter
        .resize(renderer.context.device(), [0, 480])
        .unwrap();
    assert!(matches!(
        presenter.acquire(renderer.context.device()).unwrap(),
        AcquiredFrame::Skip
    ));
    presenter
        .resize(renderer.context.device(), [800, 600])
        .unwrap();
    assert!(matches!(
        presenter.acquire(renderer.context.device()).unwrap(),
        AcquiredFrame::Recreate
    ));
    assert_replacement_capabilities(&mut presenter, renderer);
    drop(presenter);
    policy.format = wgpu::TextureFormat::R32Uint;
    let surface = renderer
        .instance
        .create_surface(renderer.window.clone())
        .unwrap();
    let capabilities = surface.get_capabilities(renderer.context.adapter());
    assert!(matches!(
        Presenter::new(surface, &capabilities, renderer.context.device(), policy),
        Err(SurfaceError::UnsupportedFormat(
            wgpu::TextureFormat::R32Uint
        ))
    ));
}

fn assert_configuration_failure_is_recoverable(
    presenter: &mut Presenter<'_>,
    device: &wgpu::Device,
) {
    let frame = match presenter.acquire(device).unwrap() {
        AcquiredFrame::Frame(frame) => frame,
        other => panic!("expected actual frame before configure failure: {other:?}"),
    };
    assert!(matches!(
        presenter.resize(device, [800, 600]),
        Err(SurfaceError::Configuration(_))
    ));
    assert!(matches!(
        presenter.acquire(device),
        Err(SurfaceError::Unconfigured)
    ));
    // wgpu invalidates the outstanding output on a failed reconfigure; discarding that
    // deliberately retained output is itself invalid and must not pollute the host callback.
    let discard_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    drop(frame);
    assert!(pollster::block_on(discard_scope.pop()).is_some());
    presenter.resize(device, [800, 600]).unwrap();
    assert!(matches!(
        presenter.acquire(device).unwrap(),
        AcquiredFrame::Frame(_)
    ));
}

fn assert_replacement_capabilities(presenter: &mut Presenter<'_>, renderer: &Renderer) {
    let surface = renderer
        .instance
        .create_surface(renderer.window.clone())
        .unwrap();
    let mut capabilities = surface.get_capabilities(renderer.context.adapter());
    capabilities.formats.clear();
    assert!(matches!(
        presenter.replace_surface(surface, &capabilities, renderer.context.device()),
        Err(SurfaceError::UnsupportedFormat(_))
    ));
    assert!(matches!(
        presenter.acquire(renderer.context.device()).unwrap(),
        AcquiredFrame::Recreate
    ));
    let surface = renderer
        .instance
        .create_surface(renderer.window.clone())
        .unwrap();
    let capabilities = surface.get_capabilities(renderer.context.adapter());
    assert!(
        presenter
            .replace_surface(surface, &capabilities, renderer.context.device())
            .unwrap()
    );
    assert!(matches!(
        presenter.acquire(renderer.context.device()).unwrap(),
        AcquiredFrame::Frame(_)
    ));
}
