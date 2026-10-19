use super::{GpuContext, REQUIRED_FEATURES};
use wgpu::{Features, TextureFormat, TextureUsages};

const SUPPORTED_FORMATS: [TextureFormat; 6] = [
    TextureFormat::Bc1RgbaUnorm,
    TextureFormat::Bc1RgbaUnormSrgb,
    TextureFormat::Bc3RgbaUnorm,
    TextureFormat::Bc3RgbaUnormSrgb,
    TextureFormat::Bc4RUnorm,
    TextureFormat::Bc5RgUnorm,
];

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request())
        .expect("GPU acceptance requires a real adapter with BC support")
}

#[test]
fn block_compression_is_required_rather_than_negotiated() {
    assert!(
        REQUIRED_FEATURES.contains(Features::TEXTURE_COMPRESSION_BC),
        "the renderer uploads BC blocks without decoding, so it must demand BC support"
    );
}

#[test]
fn a_device_can_be_acquired_on_a_host_with_a_gpu() {
    let context = context();
    let info = context.adapter().get_info();
    assert!(
        !info.name.is_empty(),
        "an acquired adapter must identify itself"
    );
    eprintln!("acquired {} via {:?}", info.name, info.backend);
}

#[test]
fn the_acquired_device_enables_the_features_that_were_required() {
    // Requesting a feature and receiving a device does not by itself prove the
    // feature is on: a device only enables what was asked for, so asking and
    // checking are two different things.
    let context = context();
    assert!(
        context.device().features().contains(REQUIRED_FEATURES),
        "the device must enable every feature the renderer requires"
    );
}

#[test]
fn every_supported_format_can_be_sampled() {
    let context = context();
    for format in SUPPORTED_FORMATS {
        let features = context.adapter().get_texture_format_features(format);
        assert!(
            features
                .allowed_usages
                .contains(TextureUsages::TEXTURE_BINDING),
            "{format:?} must be bindable as a sampled texture"
        );
    }
}

#[test]
fn a_device_that_cannot_sample_compressed_textures_is_named_in_the_error() {
    // The failure a user actually hits is "my adapter is too old". An error
    // that says so, and names the adapter, is the difference between a fix and
    // a bug report.
    let error = super::DeviceError::MissingFeatures {
        adapter: "Example Adapter".to_owned(),
        missing: Features::TEXTURE_COMPRESSION_BC,
    };
    let message = error.to_string();
    assert!(
        message.contains("Example Adapter"),
        "the error must name the adapter it examined: {message}"
    );
    assert!(
        message.contains("TEXTURE_COMPRESSION_BC"),
        "the error must name the missing feature: {message}"
    );
}

#[test]
fn an_empty_backend_instance_returns_no_adapter() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    assert!(matches!(
        pollster::block_on(GpuContext::from_instance_with_features(
            &instance,
            Features::empty()
        )),
        Err(super::DeviceError::NoAdapter)
    ));
}

#[test]
fn required_gpu_missing_features_are_not_silently_dropped() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("feature rejection acceptance requires a real adapter");
    let missing = Features::all().difference(adapter.features());
    assert!(
        !missing.is_empty(),
        "test requires some unsupported features"
    );
    let error = pollster::block_on(GpuContext::from_instance_with_features(
        &instance,
        Features::all(),
    ))
    .unwrap_err();
    match error {
        super::DeviceError::MissingFeatures {
            adapter: name,
            missing: actual,
        } => {
            assert_eq!(name, adapter.get_info().name);
            assert_eq!(actual, missing);
        }
        other => panic!("must reject requirements before requesting a device, got {other}"),
    }
}
