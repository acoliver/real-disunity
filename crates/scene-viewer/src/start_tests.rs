//! The adapter and device a viewer start makes, without a window.
use super::*;
use worldrunner_render_gpu::device::DeviceRequirements;

fn options() -> ViewerOptions {
    crate::tests::options()
}

#[test]
fn the_adapter_is_chosen_once_and_the_device_that_follows_is_made_on_it() {
    let mut gpu = SharedGpu::headless();
    let first = gpu.adapter().unwrap();
    let again = gpu.adapter().unwrap();
    assert_eq!(first.get_info().name, again.get_info().name);

    // A requirement sized to this adapter's own limits is one only this adapter can meet.
    let caller = DeviceRequirements {
        required_limits: wgpu::Limits {
            max_buffer_size: first.limits().max_buffer_size,
            ..wgpu::Limits::default()
        },
        ..DeviceRequirements::default()
    };
    let (device, authority) = gpu.device(&options(), &caller).unwrap();
    assert_eq!(
        device.limits().max_buffer_size,
        first.limits().max_buffer_size
    );
    let (_, same) = gpu
        .device(&options(), &DeviceRequirements::default())
        .unwrap();
    assert!(
        same.same_authority(&authority),
        "a later call returns the same device"
    );
}
