#[test]
fn snapshot_outputs_cannot_be_reopened_for_writing() {
    for name in [
        "opaque snapshot color",
        "opaque snapshot depth",
        "post-water depth",
    ] {
        let resource = worldrunner_render_post::ResourceRef::new(name);
        let mut producer = worldrunner_render_post::PassDeclaration::new(
            worldrunner_render_post::PassSlot::AfterOpaqueSnapshot,
        );
        producer.color = Some(worldrunner_render_post::AttachmentDeclaration {
            resource,
            format: wgpu::TextureFormat::R32Float,
            size: [32, 32],
            samples: 1,
            load: worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        });
        let mut writer = producer.clone();
        writer.slot = worldrunner_render_post::PassSlot::ScreenSpaceAfterTransparent;
        writer.color.as_mut().unwrap().load = worldrunner_render_post::AttachmentLoad::Load;
        assert_eq!(
            worldrunner_render_post::validate_frame(&[producer, writer], &[], &[]),
            Err(worldrunner_render_post::FrameError::AttachmentHazard(
                resource
            ))
        );
    }
}

use super::*;
use std::{cell::Cell, rc::Rc};
use worldrunner_render_post::{
    BuildContext, DrawContext, FrameFeature, PrepareContext, validate_frame,
};

struct Reader {
    samples: u32,
    resource: ResourceRef,
    prepared: Rc<Cell<u32>>,
}
impl FrameFeature for Reader {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::ScreenSpaceAfterTransparent);
        declaration.passes[0].samples = self.samples;
        declaration.passes[0].reads.push(self.resource);
        declaration
    }
    fn plan(
        &self,
        passes: &mut Vec<PassDeclaration>,
        _: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        let mut pass = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::Opaque)
            .unwrap()
            .clone();
        pass.slot = PassSlot::ScreenSpaceAfterTransparent;
        pass.color.as_mut().unwrap().load = AttachmentLoad::Load;
        pass.depth = None;
        if !passes.iter().any(|existing| existing.slot == pass.slot) {
            passes.push(pass);
        }
        Ok(self.declare())
    }
    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Reader {
            samples: self.samples,
            resource: self.resource,
            prepared: self.prepared.clone(),
        }))
    }
}
impl PreparedFeature for Reader {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let input = context.resource(self.resource)?;
        assert_eq!(input.size(), [32, 32]);
        assert_eq!(input.view().texture().sample_count(), 1);
        self.prepared.set(self.prepared.get() + 1);
        Ok(())
    }
    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

#[test]
fn independent_snapshot_reader_and_post_water_depth_at_one_and_four_samples() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    for samples in [1, 4] {
        let mut renderer = crate::compositor::FrameRenderer::hdr(
            &context,
            [32, 32],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
            ToneMap::Reinhard,
            None,
        )
        .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let encoder = encode_fixture(&context, &mut renderer, (&color, &depth), frame, 0.25);
        context.queue().submit([encoder.finish()]);
        assert!(
            renderer.opaque_snapshot().is_none(),
            "no reader means no snapshot passes"
        );
        let prepared = Rc::new(Cell::new(0));
        renderer
            .register(
                context.device(),
                Box::new(Reader {
                    samples,
                    resource: ResourceRef::new("opaque snapshot color"),
                    prepared: prepared.clone(),
                }),
            )
            .unwrap();
        let encoder = encode_fixture(&context, &mut renderer, (&color, &depth), frame, 0.25);
        context.queue().submit([encoder.finish()]);
        assert!(renderer.opaque_snapshot().is_none());
        let snapshot = renderer.snapshots.as_ref().unwrap()[0]
            .color
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bytes = read(&context, snapshot.texture());
        let expected: Vec<_> = [0x3400u16, 0x3000, 0x2c00, 0x3c00]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(bytes.chunks_exact(8).all(|pixel| pixel == expected));
        assert_eq!(prepared.get(), 1);
        assert_water_metadata(&context, &mut renderer, (&color, &depth), frame, samples);
        assert_eq!(prepared.get(), 2);
        assert_eq!(read(&context, snapshot.texture()), bytes_for_color(0x3400));
    }
}
fn assert_water_metadata(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    attachments: (&AttachmentView, &AttachmentView),
    frame: FrameUniform,
    samples: u32,
) {
    renderer
        .register(
            context.device(),
            Box::new(Reader {
                samples,
                resource: ResourceRef::new("post-water depth"),
                prepared: Rc::new(Cell::new(0)),
            }),
        )
        .unwrap();
    renderer
        .install_water(context.device(), settings())
        .unwrap();
    let encoder = encode_fixture(context, renderer, attachments, frame, 0.5);
    context.queue().submit([encoder.finish()]);
    let final_depth = renderer.post_water_depth().unwrap();
    let bytes = read(context, final_depth.texture());
    let offset = (16 * 32 + 16) * 4;
    let surface = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    assert!(
        (4.0..6.0).contains(&surface),
        "later metadata must describe water: {surface}"
    );
    let (snapshot, _) = renderer.opaque_snapshot().unwrap();
    assert_eq!(read(context, snapshot.texture()), bytes_for_color(0x3800));
}
pub(super) fn bytes_for_color(red: u16) -> Vec<u8> {
    (0..32 * 32)
        .flat_map(|_| [red, 0x3000, 0x2c00, 0x3c00])
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[test]
fn missing_snapshot_version_fails_before_retained_preparation() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut renderer = crate::compositor::FrameRenderer::hdr(
        &context,
        [32, 32],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        None,
    )
    .unwrap();
    let prepared = Rc::new(Cell::new(0));
    let resource = ResourceRef {
        version: 1,
        ..ResourceRef::new("opaque snapshot depth")
    };
    renderer
        .register(
            context.device(),
            Box::new(Reader {
                samples: 1,
                resource,
                prepared: prepared.clone(),
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(crate::compositor::attachment(
        color.resource(),
        &color,
        false,
    ));
    opaque.depth = Some(crate::compositor::attachment(
        depth.resource(),
        &depth,
        true,
    ));
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let result = renderer.encode_frame(
        context.device(),
        &mut encoder,
        (&[opaque], &[color, depth]),
        &mut [],
        (FrameUniform::new(&fixture_view(), None).unwrap(), None),
    );
    assert_eq!(result, Err(FrameError::MissingProducer(resource)));
    assert_eq!(prepared.get(), 0);
}

#[test]
fn unsupported_overlapping_transparent_writers_have_typed_errors() {
    let water = FeatureDeclaration::new(PassSlot::TransparentWater);
    let alpha = FeatureDeclaration::new(PassSlot::TransparentAlpha);
    let passes = [
        PassDeclaration::new(PassSlot::TransparentWater),
        PassDeclaration::new(PassSlot::TransparentAlpha),
    ];
    assert_eq!(
        validate_frame(&passes, &[water.clone(), water.clone()], &[]),
        Err(FrameError::TransparentOrdering)
    );
    assert_eq!(
        validate_frame(&passes, &[water, alpha], &[]),
        Err(FrameError::TransparentOrdering)
    );
    assert!(PassSlot::Opaque < PassSlot::AfterOpaqueSnapshot);
    assert!(PassSlot::AfterOpaqueDepth < PassSlot::TransparentWater);
    assert!(PassSlot::TransparentWater < PassSlot::AfterWaterDepth);
    assert!(PassSlot::AfterWaterDepth < PassSlot::ScreenSpaceAfterTransparent);
}
