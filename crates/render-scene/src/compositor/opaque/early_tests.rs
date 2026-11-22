use super::super::*;
use crate::compositor::ReceiverGradientReservation as Reservation;
use crate::gbuffer_writes::EarlyOutputs;
use worldrunner_render_post::{AttachmentView, FrameError, ResourceRef};

#[test]
fn render_refuses_foreign_depth_without_stencil_and_other_paths_keep_it() {
    use crate::gbuffer::{Gbuffer, GbufferUse};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let (device, _) =
        pollster::block_on(context.adapter().request_device(&wgpu::DeviceDescriptor {
            required_limits: limits(64, 8),
            ..Default::default()
        }))
        .unwrap();
    assert!(
        !device
            .features()
            .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
    );
    let foreign = crate::compositor::FrameRenderer::new(&device)
        .targets()
        .depth()
        .clone();
    let foreign = AttachmentView::new(ResourceRef::new("scene depth"), foreign.texture()).unwrap();
    for usage in [None, Some(GbufferUse::Observe), Some(GbufferUse::Render)] {
        let mut frame = crate::compositor::FrameRenderer::new(&device);
        frame.gbuffer = usage.map(|usage| {
            Gbuffer::new(
                &device,
                [1, 1],
                (wgpu::TextureFormat::Rgba16Float, false),
                usage,
            )
        });
        let own = AttachmentView::new(
            ResourceRef::new("scene depth"),
            frame.targets().depth().texture(),
        )
        .unwrap();
        assert!(frame.validate_opaque_depth(&own).is_ok());
        let result = frame.validate_opaque_depth(&foreign);
        if usage == Some(GbufferUse::Render) {
            assert!(
                matches!(result, Err(FrameError::FeaturePreparation { feature: "opaque G-buffer", reason })
                if reason == "rendering from the G-buffer needs the frame depth target")
            );
        } else {
            assert_eq!(result, Ok(()));
        }
    }
}

#[test]
fn render_allocations_exclude_receiver_payload_backing() {
    let mut settings = RendererConfiguration::new(
        [8, 8],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        worldrunner_render_post::ToneMap::Clamp,
    );
    settings.ssao = Some(crate::ssr::neutral_ao());
    settings.opaque_path = SceneOpaquePath::GBUFFER_RENDER;
    settings.receiver_gradients = Reservation::None;
    let without = settings.allocations().validate(&limits(64, 8)).unwrap();
    settings.receiver_gradients = Reservation::Front;
    assert_eq!(
        settings.allocations().validate(&limits(64, 8)).unwrap(),
        without
    );
    settings.opaque_path = SceneOpaquePath::GBUFFER_OBSERVATION;
    let with_payload = settings.allocations().validate(&limits(64, 8)).unwrap();
    settings.receiver_gradients = Reservation::None;
    assert_ne!(
        settings.allocations().validate(&limits(64, 8)).unwrap(),
        with_payload
    );
}

fn limits(bytes: u32, attachments: u32) -> wgpu::Limits {
    wgpu::Limits {
        max_color_attachment_bytes_per_sample: bytes,
        max_color_attachments: attachments,
        ..Default::default()
    }
}

#[test]
fn early_composition_checks_each_ordered_byte_and_attachment_boundary() {
    for (velocity, reflections, bytes, count) in [
        (false, false, 34, 5),
        (true, false, 38, 6),
        (false, true, 42, 6),
        (true, true, 46, 7),
    ] {
        let outputs = EarlyOutputs {
            velocity,
            reflections,
        };
        for reservation in [Reservation::None, Reservation::Front] {
            for margin in [0, 1] {
                assert!(
                    admit_early_producer(
                        SceneOpaquePath::GBUFFER_RENDER,
                        1,
                        reservation,
                        outputs,
                        &limits(bytes + margin, count + margin),
                    )
                    .is_ok()
                );
            }
        }
        let error = admit_early_producer(
            SceneOpaquePath::GBUFFER_RENDER,
            1,
            Reservation::Front,
            outputs,
            &limits(bytes - 1, count),
        )
        .unwrap_err();
        assert_reason(
            error,
            &OpaqueAdmissionError::ColorAttachmentBytes {
                required: bytes,
                available: bytes - 1,
            },
        );
        let error = admit_early_producer(
            SceneOpaquePath::GBUFFER_RENDER,
            1,
            Reservation::Front,
            outputs,
            &limits(bytes, count - 1),
        )
        .unwrap_err();
        assert_reason(
            error,
            &OpaqueAdmissionError::ColorAttachments {
                required: count,
                available: count - 1,
            },
        );
    }
}

fn assert_reason(error: PostError, expected: &OpaqueAdmissionError) {
    match error {
        PostError::Frame(FrameError::FeaturePreparation { feature, reason }) => {
            assert_eq!(feature, "opaque G-buffer");
            assert_eq!(reason, expected.to_string());
        }
        other => panic!("unexpected admission error: {other:?}"),
    }
}

#[test]
fn opposing_is_named_only_for_render_and_zero_bias_front_is_admitted() {
    assert_eq!(
        validate_receiver_reservation(SceneOpaquePath::GBUFFER_RENDER, Reservation::Opposing),
        Err(OpaqueAdmissionError::UnsupportedOpposingReceiver),
    );
    for reservation in [Reservation::None, Reservation::Front] {
        assert_eq!(
            validate_receiver_reservation(SceneOpaquePath::GBUFFER_RENDER, reservation),
            Ok(()),
        );
    }
    let outputs = EarlyOutputs {
        velocity: true,
        reflections: true,
    };
    for path in [
        SceneOpaquePath::Forward,
        SceneOpaquePath::GBUFFER_OBSERVATION,
    ] {
        assert!(
            admit_early_producer(path, 1, Reservation::Opposing, outputs, &limits(0, 0),).is_ok()
        );
    }
    assert_reason(
        admit_early_producer(
            SceneOpaquePath::GBUFFER_RENDER,
            4,
            Reservation::Front,
            outputs,
            &limits(64, 8),
        )
        .unwrap_err(),
        &OpaqueAdmissionError::UnsupportedSampleCount { observed: 4 },
    );
}

#[test]
fn configuration_preflight_is_pure_and_uses_the_composed_plan() {
    let mut settings = RendererConfiguration::new(
        [2, 2],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        worldrunner_render_post::ToneMap::Clamp,
    );
    settings.opaque_path = SceneOpaquePath::GBUFFER_RENDER;
    settings.receiver_gradients = Reservation::Front;
    let before = settings.allocations().validate(&limits(64, 8)).unwrap();
    assert_reason(
        settings.validate_opaque(&limits(33, 8)).unwrap_err(),
        &OpaqueAdmissionError::ColorAttachmentBytes {
            required: 34,
            available: 33,
        },
    );
    assert!(settings.validate_opaque(&limits(34, 5)).is_ok());
    assert_eq!(
        settings.allocations().validate(&limits(64, 8)).unwrap(),
        before
    );
}

#[test]
fn allocation_checks_enabled_device_limits_instead_of_adapter_maxima() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    assert!(
        context
            .adapter()
            .limits()
            .max_color_attachment_bytes_per_sample
            >= 34
    );
    let (device, _) =
        pollster::block_on(context.adapter().request_device(&wgpu::DeviceDescriptor {
            required_limits: limits(32, 8),
            ..Default::default()
        }))
        .unwrap();
    assert_eq!(device.limits().max_color_attachment_bytes_per_sample, 32);
    let config = FrameConfig::new([2, 2], wgpu::TextureFormat::Rgba16Float, 1);
    let error = allocate_path(&device, SceneOpaquePath::GBUFFER_RENDER, config, false)
        .err()
        .unwrap();
    assert_reason(
        error,
        &OpaqueAdmissionError::ColorAttachmentBytes {
            required: 34,
            available: 32,
        },
    );
    assert!(
        allocate_path(&device, SceneOpaquePath::Forward, config, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn configuration_preflight_includes_enabled_velocity_and_reflection_outputs() {
    for (velocity, reflections, bytes, count) in [
        (false, false, 34, 5),
        (true, false, 38, 6),
        (false, true, 42, 6),
        (true, true, 46, 7),
    ] {
        let mut settings = RendererConfiguration::new(
            [2, 2],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            worldrunner_render_post::ToneMap::Clamp,
        );
        settings.opaque_path = SceneOpaquePath::GBUFFER_RENDER;
        settings.receiver_gradients = Reservation::Front;
        if velocity {
            settings.post.resolve = worldrunner_render_post::PostResolve::taa(0.9);
        }
        if reflections {
            settings.ssr = Some(worldrunner_render_plan::SceneSsr::default());
        }
        assert!(settings.validate_opaque(&limits(bytes, count)).is_ok());
        assert_reason(
            settings
                .validate_opaque(&limits(bytes - 1, count))
                .unwrap_err(),
            &OpaqueAdmissionError::ColorAttachmentBytes {
                required: bytes,
                available: bytes - 1,
            },
        );
    }
}
