use super::Renderer;
use crate::{AdapterCallback, RetainedHost, ViewerError};
use std::sync::{Arc, Mutex};

#[test]
fn windowless_ignores_surface_support_and_reports_the_device_it_uses_once() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let received = observed.clone();
    let mut options = crate::tests::options();
    options.present_mode = wgpu::PresentMode::Mailbox;
    options.adapter_callback = Some(AdapterCallback::new(move |identity| {
        received.lock().unwrap().push(identity.clone());
        Ok(())
    }));
    let host = RetainedHost::new(crate::tests::scene(false), options).unwrap();
    let renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let observed = observed.lock().unwrap();
    assert_eq!(observed.as_slice(), &[renderer.context.adapter_identity()]);
    assert!(!observed[0].name.is_empty());
    assert_eq!(
        observed[0].backend,
        renderer.context.adapter().get_info().backend
    );
    #[cfg(target_os = "macos")]
    assert_eq!(observed[0].backend, wgpu::Backend::Metal);
    assert_eq!(
        renderer.context.device().limits().max_texture_array_layers,
        renderer.context.adapter().limits().max_texture_array_layers
    );
    eprintln!("windowless adapter: {:?}", observed[0]);
}

#[test]
fn adapter_callback_failure_stops_startup_before_scene_preparation() {
    let mut options = crate::tests::options();
    options.adapter_callback = Some(AdapterCallback::new(|_| Err("quality rejected".into())));
    let host = RetainedHost::new(crate::tests::scene(false), options).unwrap();
    assert!(matches!(
        pollster::block_on(Renderer::windowless(&host)),
        Err(ViewerError::Driver(message)) if message == "quality rejected"
    ));
}

#[test]
fn startup_surface_extent_error_preserves_size_and_device_limit() {
    let size = [16_385, 900];
    let error = worldrunner_render_gpu::surface::validate_extent(size, 16_384).unwrap_err();
    match super::surface_error(error) {
        ViewerError::SurfaceExtent { size, device_limit } => {
            assert_eq!(size, [16_385, 900]);
            assert_eq!(device_limit, 16_384);
        }
        other => panic!("expected structured startup extent error, got {other:?}"),
    }
}

#[test]
fn reconfiguration_present_mode_error_remains_typed() {
    let error = worldrunner_render_gpu::surface::SurfaceError::UnsupportedPresentMode(
        wgpu::PresentMode::Mailbox,
    );
    assert!(matches!(
        super::surface_error(error),
        ViewerError::UnsupportedPresentMode(wgpu::PresentMode::Mailbox)
    ));
}
