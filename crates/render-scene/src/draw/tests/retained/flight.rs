use super::*;
use worldrunner_render_gpu::flight::{Admission, FrameAdmission};
use worldrunner_render_post::BufferReadback;

struct Encoded {
    ticket: worldrunner_render_gpu::flight::CompletionTicket,
    instances: BufferReadback,
    uniform: BufferReadback,
    expected: Vec<u8>,
    frame_bytes: Vec<u8>,
    uniform_buffer: wgpu::Buffer,
}

#[test]
fn overlapping_frames_keep_uniform_uploads_instances_and_captures_separate() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut target = OffscreenTarget::new(
        context.device(),
        crate::Viewport {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let flight = FrameAdmission::new(2).unwrap();
    let admissions = [flight.try_acquire().unwrap(), flight.try_acquire().unwrap()];
    let frames: Vec<_> = admissions
        .into_iter()
        .enumerate()
        .map(|(index, admission)| {
            encode_frame(
                &context,
                (&mut gpu, &pipelines),
                &mut renderer,
                &mut target,
                index,
                (admission, &flight),
            )
        })
        .collect();
    assert_ne!(frames[0].uniform_buffer, frames[1].uniform_buffer);
    verify_frames(&context, &mut target, frames);
}

fn verify_frames(context: &GpuContext, target: &mut OffscreenTarget, frames: Vec<Encoded>) {
    let tickets: Vec<_> = frames
        .into_iter()
        .map(|frame| {
            let ticket = frame.ticket;
            (
                ticket,
                frame.instances,
                frame.uniform,
                frame.expected,
                frame.frame_bytes,
            )
        })
        .collect();
    for (slot, (ticket, mut instances, mut uniform, expected, bytes)) in
        tickets.into_iter().enumerate()
    {
        assert_eq!(
            instances
                .read_encoded(context.device(), Some(ticket.submission()))
                .unwrap(),
            expected
        );
        assert_eq!(
            uniform
                .read_encoded(context.device(), Some(ticket.submission()))
                .unwrap(),
            bytes
        );
        assert_eq!(
            target
                .read_encoded_slot(context, slot, ticket.submission())
                .unwrap()
                .pixels
                .len(),
            64 * 64 * 4
        );
    }
}

fn encode_frame(
    context: &GpuContext,
    scene: (&mut SceneGpu, &ScenePipelines),
    renderer: &mut crate::compositor::FrameRenderer,
    target: &mut OffscreenTarget,
    index: usize,
    admission: (Admission, &FrameAdmission),
) -> Encoded {
    let (admission, flight) = admission;
    let (gpu, pipelines) = scene;
    let slot = admission.slot();
    gpu.select_frame_slot(slot);
    renderer.select_frame_slot(slot);
    let handle = gpu.instance_handles().next().unwrap();
    let object = gpu.instance(handle).unwrap().0;
    gpu.set_instance_transform(handle, moved(if index == 0 { 1234 } else { 4321 }))
        .unwrap();
    if index == 1 {
        for _ in 0..8 {
            gpu.add_instance(object, true).unwrap();
        }
    }
    let mut frame = frame_from(pose());
    frame.exposure = if index == 0 { 1.0 } else { 2.0 };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    crate::compositor::encode_scene(
        renderer,
        context.device(),
        &mut encoder,
        (gpu, pipelines),
        (&target.color_attachment(), &target.depth_attachment()),
        frame,
    )
    .unwrap();
    let batch = gpu.instances.batches.values().next().unwrap();
    let expected = batch.bytes.clone();
    let mut instances = BufferReadback::new(context.device(), expected.len() as u64, 4096).unwrap();
    instances
        .encode(&mut encoder, batch.buffer.as_ref().unwrap())
        .unwrap();
    let mut uniform = BufferReadback::new(context.device(), gpu.frame_buffer.size(), 4096).unwrap();
    uniform.encode(&mut encoder, &gpu.frame_buffer).unwrap();
    target.encode_readback_slot(slot, &mut encoder).unwrap();
    assert!(target.encode_readback_slot(slot, &mut encoder).is_err());
    Encoded {
        ticket: flight.submit_prepared(admission, encoder.finish()).unwrap(),
        instances,
        uniform,
        expected,
        frame_bytes: frame.to_bytes().to_vec(),
        uniform_buffer: gpu.frame_buffer.clone(),
    }
}
