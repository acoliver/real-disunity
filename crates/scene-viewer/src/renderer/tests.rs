#[test]
fn device_error_observer_does_not_retain_the_context_queue() {
    let instance = worldrunner_render_gpu::device::headless_instance();
    let context = pollster::block_on(GpuContext::from_instance(&instance)).unwrap();
    let errors = super::device_errors(context.device(), context.submission());
    drop(context);
    assert!(instance.generate_report().unwrap().hub.queues.is_empty());
    assert!(instance.generate_report().unwrap().hub.devices.is_empty());
    drop(errors);
}

#[test]
fn device_error_observer_reports_loss_to_the_host_and_submission_authority() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let errors = super::device_errors(context.device(), context.submission());
    context.device().destroy();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    assert!(errors.try_recv().unwrap().contains("device lost"));
    assert!(matches!(
        context.submission().check_device(),
        Err(worldrunner_render_gpu::submission::SubmitError::DeviceLost(
            _
        ))
    ));
}

fn fxaa_recipe() -> worldrunner_render_plan::SceneHdrPost {
    worldrunner_render_plan::SceneHdrPost {
        tone_map: Some(worldrunner_render_plan::SceneToneMap::AcesFitted),
        grading: Some(worldrunner_render_plan::SceneDisplayGrading {
            linear_scale_micro: 2_000_000,
            display_power_micro: [1_250_000, 1_000_000, 750_000],
            vignette: None,
        }),
        adaptive_exposure: None,
        bloom: None,
        fxaa_subpixel_micro: Some(500_000),
        fxaa_quality: Some(worldrunner_render_plan::SceneFxaaQuality {
            edge_threshold_micro: 125_000,
            minimum_threshold_micro: 31_250,
            subpixel_blend_micro: 500_000,
            luminance: worldrunner_render_plan::SceneFxaaLuminance::Green,
            kernel: worldrunner_render_plan::SceneFxaaKernel::SecondDifference,
        }),
    }
}

fn edit_retained_settings(
    renderer: &mut Renderer,
    host: &mut crate::RetainedHost,
    replacement: &worldrunner_render_plan::SceneEffects,
    profile: crate::FrameProfile,
) {
    let expected_quality = worldrunner_render_scene::post_settings(replacement)
        .unwrap()
        .fxaa_quality;
    present_retained(renderer, host);
    let handles: Vec<_> = renderer.resident.scene_mut().instance_handles().collect();
    let original = renderer.presentation.scheduler.configuration().clone();
    if let crate::FrameProfile::HdrUpscaled { scale, upscale, .. } = profile {
        assert_eq!(original.post.upscale, Some(upscale));
        assert_eq!(
            original.frame.size,
            scale.extent(original.frame.output_extent()).unwrap()
        );
    }
    host.apply(crate::RetainedUpdate::Settings(Box::new(
        replacement.clone(),
    )))
    .unwrap();
    present_retained(renderer, host);
    assert!(host.readiness.queue.is_empty());
    assert_eq!(
        renderer
            .resident
            .scene_mut()
            .instance_handles()
            .collect::<Vec<_>>(),
        handles
    );
    let settings = renderer.presentation.scheduler.configuration();
    assert_eq!(settings.post.fxaa, Some(0.5));
    assert_eq!(settings.post.fxaa_quality, expected_quality);
    assert_eq!(settings.post.upscale, original.post.upscale);
    assert_eq!(settings.frame, original.frame);
    resize_retained_settings(renderer, host, replacement, profile, &original);
}

fn resize_retained_settings(
    renderer: &mut Renderer,
    host: &mut crate::RetainedHost,
    replacement: &worldrunner_render_plan::SceneEffects,
    profile: crate::FrameProfile,
    original: &worldrunner_render_scene::compositor::RendererConfiguration,
) {
    let expected_quality = worldrunner_render_scene::post_settings(replacement)
        .unwrap()
        .fxaa_quality;
    renderer.resize([320, 240]).unwrap();
    host.input(crate::RetainedInput::Resized([320, 240]));
    present_retained(renderer, host);
    let settings = renderer.presentation.scheduler.configuration();
    assert_eq!(settings.post.fxaa, Some(0.5));
    assert_eq!(settings.post.fxaa_quality, expected_quality);
    assert_eq!(settings.post.upscale, original.post.upscale);
    assert_eq!(settings.frame.output_extent(), [320, 240]);
    let expected = if let crate::FrameProfile::HdrUpscaled { scale, .. } = profile {
        scale.extent([320, 240]).unwrap()
    } else {
        [320, 240]
    };
    assert_eq!(settings.frame.size, expected);
    renderer.poll().unwrap();
}

fn retained_settings_present(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    use crate::{FrameProfile, RetainedHost, ToneMap};
    let scale = crate::RenderScale::new(500_000).unwrap();
    let upscale = crate::UpscaleSettings {
        mode: crate::UpscaleMode::Easu,
        sharpness: Some(0.0),
    };
    let scene = super::retained_tests::terrain_source();
    let replacement = scene
        .effects()
        .unwrap()
        .clone()
        .with_hdr_post(fxaa_recipe())
        .unwrap();
    for profile in [
        FrameProfile::Hdr {
            samples: 1,
            tone_map: ToneMap::Reinhard,
        },
        FrameProfile::HdrUpscaled {
            samples: 1,
            tone_map: ToneMap::Reinhard,
            scale,
            upscale,
        },
    ] {
        for shadows in [None, Some(super::retained_tests::shadow_settings())] {
            let mut renderer = pollster::block_on(Renderer::new(
                event_loop.owned_display_handle(),
                window.clone(),
                &scene,
                shadows,
                profile,
                2,
            ))
            .unwrap();
            let viewport = renderer.viewport.unwrap();
            let mut host = RetainedHost::new(
                scene.clone(),
                crate::ViewerOptions {
                    frame_profile: profile,
                    shadows,
                    size: [viewport.width, viewport.height],
                    ..crate::tests::options()
                },
            )
            .unwrap();
            edit_retained_settings(&mut renderer, &mut host, &replacement, profile);
        }
    }
}

#[track_caller]
fn present_retained(renderer: &mut Renderer, host: &mut crate::RetainedHost) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        renderer.poll().unwrap();
        if renderer.render_retained(host).unwrap() {
            break;
        }
        assert!(Instant::now() < deadline, "retained frame timed out");
        std::thread::yield_now();
    }
}

mod capture;
mod live;
mod recovery;
mod sdsm;
mod streaming;
mod teardown;
use super::*;
use recovery::assert_production_recovery;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::{pump_events::EventLoopExtPumpEvents, x11::EventLoopBuilderExtX11},
    window::WindowId,
};
use worldrunner_render_gpu::surface::AcquiredFrame;

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
                    crate::FrameProfile::Legacy,
                    2,
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
fn view(renderer: &Renderer) -> SceneView {
    let options = crate::tests::options();
    crate::OrbitCamera::new(options.pose, options.orbit)
        .unwrap()
        .view(options.camera, renderer.viewport.unwrap())
        .unwrap()
}
#[track_caller]
fn present(renderer: &mut Renderer) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let view = view(renderer);
    let mut update: crate::SceneUpdate = Box::new(|_| Ok(()));
    while !renderer.render(&view, &mut update).unwrap() {
        assert!(
            Instant::now() < deadline,
            "surface failed to deliver a frame"
        );
        renderer.poll().unwrap();
        std::thread::yield_now();
    }
}

#[test]
fn actual_resize_recovery_and_pending_submission_cleanup_preserve_presentation() {
    let mut event_loop = EventLoop::builder()
        .with_x11()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut harness = Harness::default();
    event_loop.pump_app_events(Some(Duration::ZERO), &mut harness);
    assert_native_resizes(&mut event_loop, &mut harness);
    let renderer = harness.renderer.as_mut().unwrap();
    let old = renderer.viewport;
    let limit = renderer.context.device().limits().max_texture_dimension_2d;
    assert!(matches!(
        renderer.resize([limit + 1, 100]),
        Err(ViewerError::SurfaceExtent { .. })
    ));
    assert_eq!(renderer.viewport, old);
    let previous_view = view(renderer);
    renderer.resize([0, 480]).unwrap();
    assert_eq!(renderer.viewport, None);
    let mut update: crate::SceneUpdate = Box::new(|_| panic!("suspended update"));
    assert!(!renderer.render(&previous_view, &mut update).unwrap());
    renderer.resize([640, 480]).unwrap();
    present(renderer);
    renderer.check_errors().unwrap();
    assert_production_recovery(renderer);
    // Additional invalid-capability diagnostics use a separate real surface.
    let window = renderer.window.as_ref().unwrap().clone();
    let device = renderer.context.device().clone();
    let adapter = renderer.context.adapter().clone();
    let queue = renderer.context.queue().clone();
    let instance = renderer.instance.clone();
    harness.renderer = None;
    let mut presenter = initial_presenter(&instance, &window, &adapter, &device);
    assert_deferred_recovery(
        &mut presenter,
        &device,
        &queue,
        &instance,
        &window,
        &adapter,
    );
    drop(presenter);
    drop((device, queue, adapter, instance));
    retained_settings_present(&event_loop, &window);
    crate::retained::session::pointer_tests::native_window_pointer_delivery(window.clone());
    streaming::streaming_present(&event_loop, &window);
    capture::retained_capture_present(&event_loop, &window);
    capture::retained_page_capture_present(&event_loop, &window);
    sdsm::retained_depth_fitting(&event_loop, &window);
    live::pending_survives_resize_and_surface_recovery(&mut event_loop, window.clone());
    teardown::assert_pending_submission_cleanup(
        pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(),
            window,
            &crate::tests::scene(false),
            None,
            crate::FrameProfile::Legacy,
            2,
        ))
        .unwrap(),
    );
}

fn assert_deferred_recovery(
    presenter: &mut Presenter<'static>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    instance: &wgpu::Instance,
    window: &Arc<Window>,
    adapter: &wgpu::Adapter,
) {
    let generation = presenter.configuration_generation();
    for _ in 0..8 {
        device.poll(wgpu::PollType::Poll).unwrap();
        let frame = match presenter.acquire(device).unwrap() {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("{other:?}"),
        };
        let frame = match presenter
            .handle_acquired(device, wgpu::CurrentSurfaceTexture::Suboptimal(frame))
            .unwrap()
        {
            AcquiredFrame::Frame(frame) => frame,
            other => panic!("{other:?}"),
        };
        queue.submit([]);
        queue.present(frame);
    }
    assert_eq!(presenter.configuration_generation(), generation);
    for outcome in [
        wgpu::CurrentSurfaceTexture::Timeout,
        wgpu::CurrentSurfaceTexture::Occluded,
    ] {
        assert!(matches!(
            presenter.handle_acquired(device, outcome).unwrap(),
            AcquiredFrame::Skip
        ));
    }
    assert!(matches!(
        presenter
            .handle_acquired(device, wgpu::CurrentSurfaceTexture::Outdated)
            .unwrap(),
        AcquiredFrame::Reconfigure
    ));
    assert_eq!(presenter.configuration_generation(), generation);
    presenter.resize(device, [700, 500]).unwrap();
    presenter.resize(device, [640, 480]).unwrap();
    assert_eq!(presenter.configuration_generation(), generation);
    presenter.apply_pending(device).unwrap();
    assert_eq!(presenter.configuration_generation(), generation + 1);
    assert_eq!(
        [
            presenter.configuration().width,
            presenter.configuration().height
        ],
        [640, 480]
    );
    assert_lost_recovery(presenter, device, instance, window, adapter, generation);
    assert_configuration_failure(presenter, device);
}

fn assert_lost_recovery(
    presenter: &mut Presenter<'static>,
    device: &wgpu::Device,
    instance: &wgpu::Instance,
    window: &Arc<Window>,
    adapter: &wgpu::Adapter,
    generation: u64,
) {
    presenter
        .handle_acquired(device, wgpu::CurrentSurfaceTexture::Lost)
        .unwrap();
    presenter.resize(device, [0, 480]).unwrap();
    assert!(matches!(
        presenter.acquire(device).unwrap(),
        AcquiredFrame::Skip
    ));
    presenter.resize(device, [640, 480]).unwrap();
    assert!(matches!(
        presenter.apply_pending(device).unwrap(),
        AcquiredFrame::Recreate
    ));
    let surface = instance.create_surface(window.clone()).unwrap();
    let mut capabilities = surface.get_capabilities(adapter);
    capabilities.formats.clear();
    assert!(matches!(
        presenter.replace_surface(surface, &capabilities, device),
        Err(SurfaceError::UnsupportedFormat(_))
    ));
    assert!(matches!(
        presenter.acquire(device).unwrap(),
        AcquiredFrame::Recreate
    ));
    let surface = instance.create_surface(window.clone()).unwrap();
    let capabilities = surface.get_capabilities(adapter);
    presenter
        .replace_surface(surface, &capabilities, device)
        .unwrap();
    assert_eq!(presenter.configuration_generation(), generation + 2);
    assert!(matches!(
        presenter.handle_acquired(device, wgpu::CurrentSurfaceTexture::Validation),
        Err(SurfaceError::Validation)
    ));
}

fn assert_configuration_failure(presenter: &mut Presenter<'static>, device: &wgpu::Device) {
    let frame = match presenter.acquire(device).unwrap() {
        AcquiredFrame::Frame(frame) => frame,
        other => panic!("{other:?}"),
    };
    presenter.resize(device, [800, 600]).unwrap();
    assert!(matches!(
        presenter.apply_pending(device),
        Err(SurfaceError::Configuration(_))
    ));
    assert!(matches!(
        presenter.acquire(device),
        Err(SurfaceError::Unconfigured)
    ));
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    drop(frame);
    assert!(pollster::block_on(scope.pop()).is_some());
    presenter.resize(device, [640, 480]).unwrap();
    presenter.apply_pending(device).unwrap();
    assert!(matches!(
        presenter.acquire(device).unwrap(),
        AcquiredFrame::Frame(_)
    ));
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
            .as_ref()
            .unwrap()
            .request_inner_size(winit::dpi::PhysicalSize::new(physical[0], physical[1]));
        let deadline = Instant::now() + Duration::from_secs(10);
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
            assert!(Instant::now() < deadline, "resize event timed out");
        }
        let renderer = harness.renderer.as_mut().unwrap();
        present(renderer);
        let texture = renderer.presentation.targets.color.texture();
        assert_eq!([texture.width(), texture.height()], intermediate);
    }
}

fn initial_presenter(
    instance: &wgpu::Instance,
    window: &Arc<Window>,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
) -> Presenter<'static> {
    let surface = instance.create_surface(window.clone()).unwrap();
    let capabilities = surface.get_capabilities(adapter);
    let mut invalid = surface_policy(&capabilities, [0, 480]).unwrap();
    invalid.format = wgpu::TextureFormat::R32Uint;
    assert!(matches!(
        Presenter::new(surface, &capabilities, device, invalid),
        Err(SurfaceError::UnsupportedFormat(
            wgpu::TextureFormat::R32Uint
        ))
    ));
    let surface = instance.create_surface(window.clone()).unwrap();
    let capabilities = surface.get_capabilities(adapter);
    let mut policy = surface_policy(&capabilities, [0, 480]).unwrap();
    policy.present_mode = wgpu::PresentMode::AutoVsync;
    policy.alpha_mode = wgpu::CompositeAlphaMode::Auto;
    let mut presenter = Presenter::new(surface, &capabilities, device, policy).unwrap();
    assert!(presenter.is_suspended());
    assert_eq!(presenter.configuration_generation(), 0);
    assert!(matches!(
        presenter.acquire(device).unwrap(),
        AcquiredFrame::Skip
    ));
    // The native resize sequence leaves the X11 window at 640x480.
    presenter.resize(device, [640, 480]).unwrap();
    assert_eq!(presenter.configuration_generation(), 0);
    presenter.apply_pending(device).unwrap();
    assert!(!presenter.is_suspended());
    assert_eq!(presenter.configuration_generation(), 1);
    presenter
}
