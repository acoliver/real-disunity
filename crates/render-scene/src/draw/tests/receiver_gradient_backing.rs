// Receiver storage must not grow when queued frames retain earlier producer layouts.
use super::*;
use crate::compositor::{ReceiverGradientReservation, RendererConfiguration};
use worldrunner_render_gpu::configuration::{AllocationBudget, Configuration};

fn configured(
    context: &GpuContext,
    reservation: ReceiverGradientReservation,
) -> (Configuration<FrameRenderer>, u64) {
    let mut settings = RendererConfiguration::legacy(SIZE, COLOR_FORMAT);
    settings.receiver_gradients = reservation;
    settings.frame.format = TextureFormat::Rgba16Float;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    let source = sloped(true);
    let effects = source.effects().unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        SIZE,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        Some(effects),
    )
    .unwrap();
    renderer.receiver_gradients = reservation;
    settings.post = crate::post::settings(effects).unwrap();
    settings.ssao = effects.ssao().copied();
    settings.ssr = effects.ssr().copied();
    let limits = context.device().limits();
    let bytes = settings.allocations().validate(&limits).unwrap();
    // The test exercises live specialization inside one allocation generation.
    let state = Configuration::new(
        context.submission(),
        renderer,
        settings.allocations(),
        &limits,
        AllocationBudget(bytes),
    )
    .unwrap();
    (state, bytes)
}

fn payload(renderer: &FrameRenderer) -> &crate::scene_metadata::receiver::ReceiverPayload {
    renderer.ssao.as_ref().unwrap().receiver.as_ref().unwrap()
}

fn submit_payload(context: &GpuContext, textures: &[Texture; 2]) -> wgpu::SubmissionIndex {
    let mut recording = context.submission().begin(context.device()).unwrap();
    for texture in textures {
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            array_layer_count: Some(1),
            ..Default::default()
        });
        let _pass = recording.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    recording.finish().submit().unwrap()
}

#[test]
fn gpu_receiver_backing_stays_within_one_reservation_while_submissions_are_outstanding() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let device = context.device();
    let source = sloped(true);
    let pipelines =
        ScenePipelines::with_ssao_taa_velocity(device, TextureFormat::Rgba16Float, 1).unwrap();
    let gpu = SceneGpu::upload(device, context.submission(), &source, &pipelines).unwrap();
    let front = SceneGpu::upload(device, context.submission(), &sloped(false), &pipelines).unwrap();
    assert!(!front.has_transmission());
    assert!(gpu.has_transmission());
    let empty = source
        .with_terrain(None)
        .unwrap()
        .with_objects_and_effects(Vec::new(), source.effects().cloned())
        .unwrap();
    let empty = SceneGpu::upload(device, context.submission(), &empty, &pipelines).unwrap();
    let (mut state, reserved) = configured(&context, ReceiverGradientReservation::Opposing);
    state
        .current_mut()
        .prepare_receiver_gradients(device, &front, 0.012)
        .unwrap();
    let backing = payload(state.current()).textures.clone();
    let submission = submit_payload(&context, &backing);
    assert!(context.submission().completed_serial() < context.submission().submitted_serial());
    let mut peak = 0;
    for (scene, bias, layout) in [
        (&gpu, 0.012, Some(ReceiverLayout::Opposing)),
        (&front, 0.012, Some(ReceiverLayout::Front)),
        (&empty, 0.012, None),
        (&gpu, 0.012, Some(ReceiverLayout::Opposing)),
        (&gpu, 0.024, Some(ReceiverLayout::Opposing)),
        (&gpu, 0.0, Some(ReceiverLayout::Front)),
    ] {
        state
            .current_mut()
            .prepare_receiver_gradients(device, scene, bias)
            .unwrap();
        let receiver = state.current().ssao.as_ref().unwrap().receiver.as_ref();
        assert_eq!(receiver.map(|payload| payload.layout), layout);
        if let Some(receiver) = receiver {
            assert_eq!(
                receiver.textures, backing,
                "live changes must retain physical backing"
            );
            assert_eq!(receiver.bias.to_bits(), bias.to_bits());
        }
        assert!(context.submission().completed_serial() < context.submission().submitted_serial());
        peak = peak.max(state.resident_bytes(&device.limits()).unwrap());
        assert_eq!(state.retired_count(), 0);
        assert_eq!(state.generation(), 0);
    }
    assert_eq!(peak, reserved);
    let merged = state
        .current_mut()
        .ssao
        .as_mut()
        .unwrap()
        .geometry
        .merged
        .take();
    state
        .current_mut()
        .prepare_receiver_gradients(device, &gpu, 0.024)
        .unwrap();
    assert_eq!(payload(state.current()).textures, backing);
    state.current_mut().ssao.as_mut().unwrap().geometry.merged = merged;
    state
        .current_mut()
        .prepare_receiver_gradients(device, &gpu, 0.024)
        .unwrap();
    assert_eq!(payload(state.current()).textures, backing);
    assert_eq!(backing[0].depth_or_array_layers(), 2);
    assert_eq!(state.resident_bytes(&device.limits()).unwrap(), reserved);
    assert_eq!(state.retired_count(), 0);
    assert!(context.submission().completed_serial() < context.submission().submitted_serial());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    state.collect();
    assert_eq!(state.retired_count(), 0);
    assert_eq!(state.resident_bytes(&device.limits()).unwrap(), reserved);
}

#[test]
fn gpu_receiver_front_reservation_retains_only_one_layer_across_bias_changes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let device = context.device();
    let pipelines = ScenePipelines::with_ssao(device, TextureFormat::Rgba16Float, 1).unwrap();
    let gpu = SceneGpu::upload(device, context.submission(), &sloped(false), &pipelines).unwrap();
    let (mut state, reserved) = configured(&context, ReceiverGradientReservation::Front);
    state
        .current_mut()
        .prepare_receiver_gradients(device, &gpu, 0.0)
        .unwrap();
    let backing = payload(state.current()).textures.clone();
    let submission = submit_payload(&context, &backing);
    for bias in [0.012, 0.024, 0.0] {
        state
            .current_mut()
            .prepare_receiver_gradients(device, &gpu, bias)
            .unwrap();
        assert_eq!(payload(state.current()).textures, backing);
        for texture in &backing {
            assert_eq!(texture.depth_or_array_layers(), 1);
        }
        assert_eq!(state.resident_bytes(&device.limits()).unwrap(), reserved);
        assert_eq!(state.retired_count(), 0);
    }
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
}
