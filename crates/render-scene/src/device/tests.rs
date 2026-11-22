#[test]
fn scene_device_requests_adapter_array_capacity_and_preserves_other_limits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let available = context.adapter().limits();
    let mut actual = context.device().limits();
    assert_eq!(
        actual.max_texture_array_layers,
        available.max_texture_array_layers
    );
    assert_eq!(
        actual.max_color_attachment_bytes_per_sample,
        available.max_color_attachment_bytes_per_sample.min(64)
    );
    actual.max_color_attachment_bytes_per_sample =
        wgpu::Limits::default().max_color_attachment_bytes_per_sample;
    actual.max_texture_array_layers = wgpu::Limits::default().max_texture_array_layers;
    assert_eq!(
        actual.max_storage_textures_per_shader_stage,
        available.max_storage_textures_per_shader_stage.min(16),
        "a single-pass downsampler binds seven storage images"
    );
    actual.max_sampled_textures_per_shader_stage =
        wgpu::Limits::default().max_sampled_textures_per_shader_stage;
    actual.max_storage_textures_per_shader_stage =
        wgpu::Limits::default().max_storage_textures_per_shader_stage;
    assert_eq!(actual, wgpu::Limits::default());
}

#[test]
fn adapter_classification_covers_every_device_type() {
    use super::AdapterClass;
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let info = context.adapter().get_info();
    let identity = context.adapter_identity();
    assert_eq!(identity.name, info.name);
    assert_eq!(identity.backend, info.backend);
    assert_eq!(identity.class, super::AdapterClass::from(info.device_type));
    if info.device_type == wgpu::DeviceType::Cpu {
        assert_eq!(identity.class, super::AdapterClass::Software);
    }
    if info.name.to_lowercase().contains("llvmpipe") {
        assert_eq!(identity.class, super::AdapterClass::Software);
        // A software rasterizer has no PCI device, so nothing names its memory.
        assert_eq!(identity.memory, None);
    }
    assert_eq!(
        identity.memory,
        worldrunner_render_gpu::adapter_memory::AdapterMemory::probe(&info)
    );
    eprintln!(
        "adapter {}: {:?} via {:?}, memory {:?}",
        identity.name, identity.class, info.backend, identity.memory
    );
}

#[test]
fn dropping_context_releases_its_queue_without_waiting_for_device_loss() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = worldrunner_render_gpu::test_support::test_instance();
    for _ in 0..3 {
        let context = pollster::block_on(GpuContext::from_instance(
            &instance,
            &DeviceRequest {
                features: REQUIRED_FEATURES,
                ..DeviceRequest::default()
            },
        ))
        .unwrap();
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
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = worldrunner_render_gpu::test_support::test_instance();
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
        unavailable_optional_limits: Vec::new(),
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
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = worldrunner_render_gpu::test_support::test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let available = adapter.features().contains(Features::TIMESTAMP_QUERY);
    let context = pollster::block_on(GpuContext::from_adapter(
        adapter,
        &DeviceRequest {
            features: Features::empty(),
            timing: TimingMode::Optional,
            ..DeviceRequest::default()
        },
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

#[test]
fn a_device_shared_with_another_user_meets_its_requirements_and_the_scenes() {
    use worldrunner_render_gpu::device::DeviceRequirements;
    use worldrunner_render_post::TimingMode;
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = worldrunner_render_gpu::test_support::test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let available = adapter.limits();
    // A caller that needs larger buffers than the default, and prefers more storage images.
    let buffer = (wgpu::Limits::default().max_buffer_size * 2).min(available.max_buffer_size);
    let caller = DeviceRequirements {
        required_limits: wgpu::Limits {
            max_buffer_size: buffer,
            ..wgpu::Limits::default()
        },
        optional_features: Features::FLOAT32_FILTERABLE,
        ..DeviceRequirements::default()
    };
    let filterable = adapter.features().contains(Features::FLOAT32_FILTERABLE);
    let context = pollster::block_on(GpuContext::from_adapter(
        adapter,
        &DeviceRequest {
            features: Features::empty(),
            timing: TimingMode::Disabled,
            shared: caller.clone(),
        },
    ))
    .unwrap();
    let limits = context.device().limits();
    assert_eq!(limits.max_buffer_size, buffer, "the caller's limit");
    assert_eq!(
        limits.max_texture_array_layers, available.max_texture_array_layers,
        "the scene's limit"
    );
    assert_eq!(
        context
            .device()
            .features()
            .contains(Features::FLOAT32_FILTERABLE),
        filterable,
        "the caller's optional feature where offered"
    );
}

#[test]
fn a_shared_device_reports_a_preferred_profile_it_could_not_grant() {
    use worldrunner_render_gpu::device::DeviceRequirements;
    use worldrunner_render_post::TimingMode;
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = worldrunner_render_gpu::test_support::test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let too_large = adapter.limits().max_texture_dimension_2d + 1;
    let caller = DeviceRequirements {
        optional_limits: Some(wgpu::Limits {
            max_texture_dimension_2d: too_large,
            ..wgpu::Limits::default()
        }),
        ..DeviceRequirements::default()
    };
    let context = pollster::block_on(GpuContext::from_adapter(
        adapter,
        &DeviceRequest {
            features: Features::empty(),
            timing: TimingMode::Disabled,
            shared: caller.clone(),
        },
    ))
    .unwrap();
    let report = context.unavailable_optional_limits();
    assert!(
        report
            .iter()
            .any(|line| line.contains("max_texture_dimension_2d")),
        "the refused preferred limit is named: {report:?}"
    );
    assert!(
        context.device().limits().max_texture_dimension_2d < too_large,
        "the device has the required limits"
    );
}

#[test]
fn a_shared_device_refuses_a_callers_missing_feature_by_name() {
    use worldrunner_render_gpu::device::DeviceRequirements;
    use worldrunner_render_post::TimingMode;
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = worldrunner_render_gpu::test_support::test_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let name = adapter.get_info().name;
    let flag = Features::all()
        .difference(adapter.features())
        .features_wgpu
        .iter()
        .next()
        .expect("no adapter offers every native feature");
    let mut absent = Features::empty();
    absent.features_wgpu = flag;
    let caller = DeviceRequirements {
        required_features: absent,
        ..DeviceRequirements::default()
    };
    let refused = pollster::block_on(GpuContext::from_adapter(
        adapter,
        &DeviceRequest {
            features: Features::empty(),
            timing: TimingMode::Disabled,
            shared: caller.clone(),
        },
    ));
    match refused {
        Err(super::DeviceError::MissingFeatures { adapter, missing }) => {
            assert_eq!(adapter, name);
            assert_eq!(missing, absent);
        }
        other => panic!("expected the caller's missing feature by name, got {other:?}"),
    }
}

use super::{DeviceRequest, GpuContext, REQUIRED_FEATURES};
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
    pollster::block_on(GpuContext::request_for_test())
        .expect("GPU acceptance requires a real adapter with BC support")
}

#[test]
fn block_compression_is_required_rather_than_negotiated() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert!(
        REQUIRED_FEATURES.contains(Features::TEXTURE_COMPRESSION_BC),
        "the renderer uploads BC blocks without decoding, so it must demand BC support"
    );
}

#[test]
fn a_device_can_be_acquired_on_a_host_with_a_gpu() {
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    assert!(
        context.device().features().contains(REQUIRED_FEATURES),
        "the device must enable every feature the renderer requires"
    );
}

#[test]
fn every_supported_format_can_be_sampled() {
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    assert!(matches!(
        pollster::block_on(GpuContext::from_instance(
            &instance,
            &DeviceRequest {
                features: Features::empty(),
                ..DeviceRequest::default()
            }
        )),
        Err(super::DeviceError::NoAdapter)
    ));
}

#[test]
fn required_gpu_missing_features_are_not_silently_dropped() {
    let _session = crate::test_support::SceneTestSession::acquire();
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
    let error = pollster::block_on(GpuContext::from_instance(
        &instance,
        &DeviceRequest {
            features: Features::all(),
            ..DeviceRequest::default()
        },
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

#[test]
fn the_scene_device_asks_for_float32_filtering_where_the_adapter_offers_it() {
    use worldrunner_render_gpu::device::negotiate;
    use worldrunner_render_post::TimingMode;
    let limits = wgpu::Limits::default();
    let request = super::requirements(&limits, REQUIRED_FEATURES, TimingMode::Disabled);
    for offered in [false, true] {
        let available = REQUIRED_FEATURES
            | Features::DEPTH32FLOAT_STENCIL8
            | if offered {
                Features::FLOAT32_FILTERABLE
            } else {
                Features::empty()
            };
        let negotiated = negotiate(available, &limits, &request)
            .expect("an adapter without the optional feature still gets a device");
        assert_eq!(
            negotiated
                .enabled_features
                .contains(Features::FLOAT32_FILTERABLE),
            offered,
            "FLOAT32_FILTERABLE is enabled exactly where offered: {:?}",
            negotiated.enabled_features
        );
        assert!(
            negotiated
                .enabled_features
                .contains(REQUIRED_FEATURES | Features::DEPTH32FLOAT_STENCIL8),
            "the other features are unchanged: {:?}",
            negotiated.enabled_features
        );
    }
}

/// A shader that samples an `R32Float` texture (such as a copy of device depth) through a linear
/// sampler declares it `Float { filterable: true }`. wgpu treats 32-bit float formats as
/// unfilterable unless the device enabled `FLOAT32_FILTERABLE`, whatever the adapter reports.
#[test]
fn the_engine_device_binds_r32float_to_a_filtering_sampler_where_the_adapter_offers_it() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let offered = context
        .adapter()
        .features()
        .contains(Features::FLOAT32_FILTERABLE);
    let device = context.device();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("r32float filtered"),
        size: wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::R32Float,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("linear clamp"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..wgpu::SamplerDescriptor::default()
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("filterable float and filtering sampler"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("r32float through linear"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let error = pollster::block_on(scope.pop());
    assert_eq!(
        error.is_none(),
        offered,
        "an R32Float view binds to a filterable binding exactly where FLOAT32_FILTERABLE is \
         offered: {error:?}"
    );
    assert_eq!(
        device.features().contains(Features::FLOAT32_FILTERABLE),
        offered,
        "the engine device enables FLOAT32_FILTERABLE exactly where the adapter offers it"
    );
}
