#[test]
fn adapter_classification_covers_every_device_type() {
    use super::AdapterClass;
    for (kind, expected) in [
        (wgpu::DeviceType::DiscreteGpu, AdapterClass::Discrete),
        (wgpu::DeviceType::IntegratedGpu, AdapterClass::Integrated),
        (wgpu::DeviceType::VirtualGpu, AdapterClass::Virtual),
        (wgpu::DeviceType::Cpu, AdapterClass::Software),
        (wgpu::DeviceType::Other, AdapterClass::Other),
    ] {
        assert_eq!(AdapterClass::from(kind), expected);
    }
}

#[test]
fn selected_adapter_identity_matches_actual_device() {
    let context = context();
    let info = context.adapter().get_info();
    let identity = context.adapter_identity();
    assert_eq!(identity.name, info.name);
    assert_eq!(identity.class, super::AdapterClass::from(info.device_type));
    if info.device_type == wgpu::DeviceType::Cpu {
        assert_eq!(identity.class, super::AdapterClass::Software);
    }
    if info.name.to_lowercase().contains("llvmpipe") {
        assert_eq!(identity.class, super::AdapterClass::Software);
    }
    eprintln!(
        "adapter {}: {:?} via {:?}",
        identity.name, identity.class, info.backend
    );
}

#[test]
fn dropping_context_releases_its_queue_without_waiting_for_device_loss() {
    let instance = worldrunner_render_gpu::device::headless_instance();
    for _ in 0..3 {
        let context = pollster::block_on(GpuContext::from_instance(&instance)).unwrap();
        assert_eq!(
            instance
                .generate_report()
                .unwrap()
                .hub
                .queues
                .num_kept_from_user,
            1
        );
        drop(context);
        assert!(instance.generate_report().unwrap().hub.queues.is_empty());
        assert!(instance.generate_report().unwrap().hub.devices.is_empty());
    }
}

#[test]
fn device_loss_callback_still_rejects_submissions() {
    let context = context();
    context.device().destroy();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        })
        .unwrap();
    assert!(matches!(
        context.submission().check_device(),
        Err(worldrunner_render_gpu::submission::SubmitError::DeviceLost(
            _
        ))
    ));
}

#[test]
fn hdr_rejects_extent_invalid_bloom_before_buffer_allocation() {
    use crate::{compositor::FrameRenderer, post_bloom_tests};
    use worldrunner_render_post::{PostError, ToneMap};
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_buffer_size: 8,
            ..Default::default()
        },
        ..Default::default()
    }))
    .unwrap();
    let context = GpuContext {
        adapter,
        device,
        submission: worldrunner_render_gpu::submission::SubmissionAuthority::new(queue),
    };
    let mut bloom = post_bloom_tests::bloom();
    bloom.recipe.as_mut().unwrap().terminal = worldrunner_render_plan::SceneBloomTerminal::Reject;
    let effects = post_bloom_tests::effects(bloom);
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let result = FrameRenderer::hdr(
        &context,
        [2, 2],
        TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        Some(&effects),
    );
    assert!(matches!(
        result,
        Err(PostError::UnsupportedProfile(
            "Gaussian bloom levels exceed extent"
        ))
    ));
    context.device().poll(wgpu::PollType::Poll).unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
    // The HDR factory's 16-byte buffers must exceed this device's limit.
    let scope = context
        .device()
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let _buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("preflight allocation sentinel"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    context.device().poll(wgpu::PollType::Poll).unwrap();
    assert!(pollster::block_on(scope.pop()).is_some());
}

#[test]
fn timestamp_negotiation_reports_optional_and_rejects_required_absence() {
    use worldrunner_render_gpu::device::{DeviceRequirements, negotiate};
    use worldrunner_render_post::TimingMode;
    let limits = wgpu::Limits::default();
    for available in [Features::empty(), Features::TIMESTAMP_QUERY] {
        let optional = negotiate(
            available,
            &limits,
            &DeviceRequirements {
                optional_features: TimingMode::Optional.features(),
                ..DeviceRequirements::default()
            },
        )
        .unwrap();
        assert_eq!(optional.enabled_features, available);
        assert_eq!(
            optional.unavailable_optional,
            Features::TIMESTAMP_QUERY.difference(available)
        );
        let required = negotiate(
            available,
            &limits,
            &DeviceRequirements {
                required_features: TimingMode::Required.features(),
                ..DeviceRequirements::default()
            },
        );
        assert_eq!(
            required.is_ok(),
            available.contains(Features::TIMESTAMP_QUERY)
        );
    }
}

#[test]
fn gpu_optional_timestamp_device_enables_only_available_capability() {
    use worldrunner_render_post::{PassTimer, TimingMode, TimingSupport};
    let instance = worldrunner_render_gpu::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let available = adapter.features().contains(Features::TIMESTAMP_QUERY);
    let context = pollster::block_on(GpuContext::from_adapter_with_timing(
        adapter,
        Features::empty(),
        TimingMode::Optional,
    ))
    .unwrap();
    assert_eq!(
        context
            .device()
            .features()
            .contains(Features::TIMESTAMP_QUERY),
        available
    );
    assert!(
        !context
            .device()
            .features()
            .contains(Features::TIMESTAMP_QUERY_INSIDE_PASSES)
    );
    let timer = PassTimer::new(context.device(), context.queue(), TimingMode::Optional).unwrap();
    assert_eq!(
        timer.support(),
        if available {
            TimingSupport::Supported
        } else {
            TimingSupport::Unsupported
        }
    );
    assert_eq!(
        PassTimer::new(context.device(), context.queue(), TimingMode::Required).is_ok(),
        available
    );
}

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
