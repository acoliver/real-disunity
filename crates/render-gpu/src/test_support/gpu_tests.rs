use super::TestGpu;
use crate::device::{DeviceRequirements, headless_instance, request_device};

#[test]
fn gpu_cached_handles_preserve_exact_device_profiles() {
    let requirements = DeviceRequirements::default();
    let first = TestGpu::shared(&requirements).unwrap();
    let second = TestGpu::shared(&requirements).unwrap();
    assert_eq!(first.device, second.device);
    assert_eq!(first.queue, second.queue);
    let reduced = TestGpu::shared(&DeviceRequirements {
        required_limits: wgpu::Limits {
            max_bind_groups: 3,
            ..Default::default()
        },
        ..Default::default()
    })
    .unwrap();
    assert_ne!(first.device, reduced.device);
    assert_eq!(reduced.device.limits().max_bind_groups, 3);
    assert_eq!(first.device.features(), wgpu::Features::empty());
}

#[test]
fn gpu_production_requests_still_create_fresh_devices() {
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let first =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default())).unwrap();
    let second =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default())).unwrap();
    let cached = TestGpu::shared(&DeviceRequirements::default()).unwrap();
    assert_ne!(first.0, second.0);
    assert_ne!(first.0, cached.device);
    assert_ne!(second.0, cached.device);
}
