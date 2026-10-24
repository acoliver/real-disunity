use super::*;
use worldrunner_render_gpu::submission::{PrepareError, PreparedFrame, SubmitError};

fn prepare(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    renderer: &mut crate::compositor::FrameRenderer,
    target: &OffscreenTarget,
) -> PreparedFrame {
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            crate::compositor::encode_scene(
                renderer,
                context.device(),
                encoder,
                (gpu, pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
                frame_from(pose()),
            )
        })
        .unwrap()
}

#[test]
fn cancelling_resident_uploads_preserves_pending_edits_until_submission() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let target = retained_target(&context);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let handle = gpu.instance_handles().next().unwrap();
    gpu.set_instance_transform(handle, moved(1234)).unwrap();
    let pending = pending_ranges(&gpu);
    let frame = prepare(&context, &mut gpu, &pipelines, &mut renderer, &target);
    assert_eq!(gpu.current_frame(), None);
    assert_eq!(
        gpu.instances
            .batches
            .values()
            .next()
            .unwrap()
            .pending_ranges(),
        pending
    );
    assert_busy(&context, &mut gpu, &pipelines);
    frame.cancel();
    assert_eq!(gpu.current_frame(), None);
    let frame = prepare(&context, &mut gpu, &pipelines, &mut renderer, &target);
    drop(frame);
    assert_eq!(
        gpu.instances
            .batches
            .values()
            .next()
            .unwrap()
            .pending_ranges(),
        pending
    );
    let submitted = prepare(&context, &mut gpu, &pipelines, &mut renderer, &target)
        .submit()
        .unwrap();
    assert!(gpu.current_frame().is_some());
    assert!(
        gpu.instances
            .batches
            .values()
            .next()
            .unwrap()
            .pending_ranges()
            .is_empty()
    );
    let next = prepare(&context, &mut gpu, &pipelines, &mut renderer, &target);
    next.cancel();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: None,
        })
        .unwrap();
}

fn target_busy(
    context: &GpuContext,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
) -> Result<crate::offscreen::RgbaImage, crate::offscreen::OffscreenError> {
    retained_target(context).render(context, gpu, pipelines)
}

fn assert_busy(context: &GpuContext, gpu: &mut SceneGpu, pipelines: &ScenePipelines) {
    assert!(matches!(
        context
            .submission()
            .prepare(context.device(), |_, _| Ok::<_, ()>(())),
        Err(PrepareError::Busy)
    ));
    assert!(matches!(
        gpu.flush_instances(context.device(), context.submission()),
        Err(SubmitError::Busy)
    ));
    assert!(matches!(
        target_busy(context, gpu, pipelines),
        Err(crate::offscreen::OffscreenError::Submission(
            SubmitError::Busy
        ))
    ));
}

fn pending_ranges(gpu: &SceneGpu) -> Vec<std::ops::Range<u32>> {
    gpu.instances
        .batches
        .values()
        .next()
        .unwrap()
        .pending_ranges()
}

#[test]
fn cancelled_camera_does_not_change_a_later_resident_draw() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut target = retained_target(&context);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    prepare(&context, &mut gpu, &pipelines, &mut renderer, &target)
        .submit()
        .unwrap();
    let expected = target.render(&context, &gpu, &pipelines).unwrap();
    let old_stats = gpu.view_stats();
    gpu.select_frame_slot(1);
    let mut camera = frame_from(pose());
    camera.view_projection = glam::Mat4::from_translation(glam::Vec3::splat(1000.0));
    let cancelled = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            crate::compositor::encode_scene(
                &mut renderer,
                context.device(),
                encoder,
                (&mut gpu, &pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
                camera,
            )
        })
        .unwrap();
    cancelled.cancel();
    assert_eq!(
        target.render(&context, &gpu, &pipelines).unwrap().pixels,
        expected.pixels
    );
    assert_eq!(gpu.view_stats(), old_stats);
}

#[test]
fn cancelled_growth_with_empty_sparse_receipt_reinitializes_the_buffer() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let expected = gpu.instances.batches.values().next().unwrap().bytes.clone();
    let object = source.geometry().objects()[0];
    let added: Vec<_> = (0..16)
        .map(|_| gpu.add_instance(object, true).unwrap())
        .collect();
    let cancelled = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            gpu.flush_instances_encoded(context.device(), encoder)
                .map(|_| ())
        })
        .unwrap();
    cancelled.cancel();
    for handle in added {
        gpu.remove_instance(handle).unwrap();
    }
    assert!(pending_ranges(&gpu).is_empty());
    assert!(
        gpu.flush_instances(context.device(), context.submission())
            .unwrap()
            .is_empty()
    );
    let batch = gpu.instances.batches.values().next().unwrap();
    assert!(!batch.has_pending());
    assert_eq!(
        super::scheduler::buffer_bytes(
            &context,
            batch.buffer.as_ref().unwrap(),
            expected.len() as u64
        ),
        expected
    );
}

#[test]
fn owned_statistics_can_be_retained_across_publication() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let target = retained_target(&context);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let prepared = prepare(&context, &mut gpu, &pipelines, &mut renderer, &target);
    let previous = gpu.view_stats();
    let shadow = gpu.shadow_view_stats();
    prepared.submit().unwrap();
    assert_eq!(previous.considered, 0);
    assert_eq!(shadow.considered, 0);
    assert_eq!(gpu.view_stats().considered, 2);
}

#[test]
fn resident_upload_rejects_standalone_and_foreign_encoders() {
    use worldrunner_render_gpu::submission::TransactionError;
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    gpu.set_instance_transform(handle, moved(4321)).unwrap();
    let mut foreign = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert_eq!(
        gpu.flush_instances_encoded(context.device(), &mut foreign)
            .unwrap_err(),
        TransactionError::Missing
    );
    let rejected = context.submission().prepare(context.device(), |_, _| {
        gpu.flush_instances_encoded(context.device(), &mut foreign)
            .map(|_| ())
    });
    assert!(matches!(
        rejected,
        Err(PrepareError::Preparation(TransactionError::ForeignEncoder))
    ));
    assert!(!pending_ranges(&gpu).is_empty());
    gpu.flush_instances(context.device(), context.submission())
        .unwrap();
    assert!(pending_ranges(&gpu).is_empty());
}

#[test]
fn abandoned_standalone_draw_does_not_publish_selection() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let mut target = retained_target(&context);
    let before = gpu.view_stats();
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    record_scene(
        &mut encoder,
        target.color_view(),
        target.depth_view(),
        &gpu,
        &pipelines,
    );
    assert_eq!(gpu.view_stats(), before);
    assert!(gpu.camera_selection.borrow().ranges.is_empty());
    drop(encoder);
    assert_eq!(gpu.view_stats(), before);
    assert_eq!(gpu.shadow_view_stats().considered, 0);
    target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().considered, 2);
    assert!(!gpu.camera_selection.borrow().ranges.is_empty());
}

#[test]
fn foreign_raw_draw_does_not_publish_with_another_encoders_submission() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let mut target = retained_target(&context);
    let before = gpu.view_stats();
    let mut foreign = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let prepared = context
        .submission()
        .prepare(context.device(), |_, _| {
            record_scene(
                &mut foreign,
                target.color_view(),
                target.depth_view(),
                &gpu,
                &pipelines,
            );
            Ok::<_, ()>(())
        })
        .unwrap();
    assert_eq!(gpu.view_stats(), before);
    drop(foreign);
    let submission = prepared.submit().unwrap();
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    assert_eq!(gpu.view_stats(), before);
    assert!(gpu.camera_selection.borrow().ranges.is_empty());
    assert_eq!(gpu.shadow_view_stats().considered, 0);
    target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(gpu.view_stats().considered, 2);
    assert!(!gpu.camera_selection.borrow().ranges.is_empty());
}
