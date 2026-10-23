use super::*;
use std::{cell::Cell, rc::Rc};
use worldrunner_render_post::{
    BufferReadback, DrawContext, FeatureDeclaration, FrameError, FrameFeature, PassSlot,
    PrepareContext, PreparedFeature,
};

struct FailingFeature {
    fail: Rc<Cell<bool>>,
    sentinel: wgpu::Buffer,
}
impl FrameFeature for FailingFeature {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration::new(PassSlot::Opaque)
    }
    fn build(
        &self,
        _: &worldrunner_render_post::BuildContext<'_>,
    ) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Self {
            fail: self.fail.clone(),
            sentinel: self.sentinel.clone(),
        }))
    }
}
impl PreparedFeature for FailingFeature {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        context.upload_buffer(&self.sentinel, 0, &[255; 4]);
        if self.fail.get() {
            return Err(FrameError::UnsupportedEffect("injected"));
        }
        Ok(())
    }
    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

fn buffer_bytes(context: &GpuContext, buffer: &wgpu::Buffer, bytes: u64) -> Vec<u8> {
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut readback = BufferReadback::new(context.device(), bytes, 4096).unwrap();
    readback.encode(&mut encoder, buffer).unwrap();
    let submission = context.queue().submit([encoder.finish()]);
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}

#[test]
fn stock_growth_survives_later_preparation_failure_without_encoder_commands() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 2);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(context.device(), context.queue(), &source, &pipelines).unwrap();
    let target = OffscreenTarget::new(
        context.device(),
        crate::Viewport {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let fail = Rc::new(Cell::new(true));
    let sentinel = register_failure(&context, &mut renderer, &fail);
    let handle = gpu.instance_handles().next().unwrap();
    let old_buffer = gpu
        .instances
        .batches
        .values()
        .next()
        .unwrap()
        .buffer
        .clone()
        .unwrap();
    let old_bytes = buffer_bytes(&context, &old_buffer, old_buffer.size());
    gpu.set_instance_transform(handle, moved(1234)).unwrap();
    for _ in 0..8 {
        gpu.add_instance(source.geometry().objects()[0], true)
            .unwrap();
    }
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let frame = frame_from(pose());
    assert_eq!(
        crate::compositor::encode_scene(
            &mut renderer,
            context.device(),
            &mut encoder,
            (&mut gpu, &pipelines),
            (&target.color_attachment(), &target.depth_attachment()),
            frame,
        ),
        Err(FrameError::UnsupportedEffect("injected"))
    );
    let batch = gpu.instances.batches.values().next().unwrap();
    assert_eq!(batch.buffer.as_ref().unwrap(), &old_buffer);
    assert_eq!(batch.pending_ranges(), [0..1, 2..10]);
    assert_eq!(gpu.frame.get(), None);
    // Submitting the failed encoder proves that even staged uploads were not appended.
    context.queue().submit([encoder.finish()]);
    assert_eq!(buffer_bytes(&context, &sentinel, 4), [0; 4]);
    assert_eq!(
        buffer_bytes(&context, &old_buffer, old_buffer.size()),
        old_bytes
    );
    discard_failed_frame(&context, &mut gpu, &pipelines, &mut renderer, &target);
    fail.set(false);
    assert_retry(&context, &mut gpu, &pipelines, &mut renderer, &target);
}

#[test]
fn invalid_plain_contract_does_not_publish_frame_or_consume_uploads() {
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 1);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(context.device(), context.queue(), &source, &pipelines).unwrap();
    let target = OffscreenTarget::new(
        context.device(),
        crate::Viewport {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let wrong = worldrunner_render_post::color_texture(
        context.device(),
        "wrong",
        [16, 16],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let color = worldrunner_render_post::AttachmentView::new(
        worldrunner_render_post::ResourceRef::new("scene color"),
        &wrong,
    )
    .unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    gpu.set_instance_transform(handle, moved(1234)).unwrap();
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let before = buffer_bytes(&context, &gpu.frame_buffer, gpu.frame_buffer.size());
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(matches!(
        crate::compositor::encode_scene(
            &mut renderer,
            context.device(),
            &mut encoder,
            (&mut gpu, &pipelines),
            (&color, &target.depth_attachment()),
            frame_from(pose()),
        ),
        Err(FrameError::AttachmentContract(PassSlot::Opaque))
    ));
    assert_eq!(gpu.frame.get(), None);
    context.queue().submit([encoder.finish()]);
    assert_eq!(
        buffer_bytes(&context, &gpu.frame_buffer, gpu.frame_buffer.size()),
        before
    );
    assert!(gpu.instances.batches.values().next().unwrap().has_pending());
}

fn assert_retry(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    renderer: &mut crate::compositor::FrameRenderer,
    target: &OffscreenTarget,
) {
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    crate::compositor::encode_scene(
        renderer,
        context.device(),
        &mut encoder,
        (gpu, pipelines),
        (&target.color_attachment(), &target.depth_attachment()),
        frame_from(pose()),
    )
    .unwrap();
    context.queue().submit([encoder.finish()]);
    let batch = gpu.instances.batches.values().next().unwrap();
    assert!(batch.pending_ranges().is_empty());
    assert_eq!(
        buffer_bytes(
            context,
            batch.buffer.as_ref().unwrap(),
            batch.bytes.len() as u64
        ),
        batch.bytes
    );
}

fn register_failure(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    fail: &Rc<Cell<bool>>,
) -> wgpu::Buffer {
    let sentinel = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    renderer
        .register(
            context.device(),
            Box::new(FailingFeature {
                fail: fail.clone(),
                sentinel: sentinel.clone(),
            }),
        )
        .unwrap();
    sentinel
}

fn discard_failed_frame(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    renderer: &mut crate::compositor::FrameRenderer,
    target: &OffscreenTarget,
) {
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert_eq!(
        crate::compositor::encode_scene(
            renderer,
            context.device(),
            &mut encoder,
            (gpu, pipelines),
            (&target.color_attachment(), &target.depth_attachment()),
            frame_from(pose())
        ),
        Err(FrameError::UnsupportedEffect("injected"))
    );
    drop(encoder);
}
