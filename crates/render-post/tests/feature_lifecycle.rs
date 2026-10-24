use std::{cell::Cell, rc::Rc};
use worldrunner_render_post::*;

struct Counted {
    samples: u32,
    counts: Rc<Cell<[u32; 3]>>,
}
impl FrameFeature for Counted {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0].samples = self.samples;
        declaration
    }
    fn build(&self, _: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        let [build, prepare, draw] = self.counts.get();
        self.counts.set([build + 1, prepare, draw]);
        Ok(Box::new(Self {
            samples: self.samples,
            counts: self.counts.clone(),
        }))
    }
}
impl PreparedFeature for Counted {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let [build, prepare, draw] = self.counts.get();
        self.counts.set([build, prepare + 1, draw]);
        Ok(())
    }
    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        let [build, prepare, draw] = self.counts.get();
        self.counts.set([build, prepare, draw + 1]);
    }
}

#[test]
fn invalid_feature_contract_is_rejected_before_build_and_after_resize() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let config = FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::new(&device, config.format),
    )
    .unwrap();
    let counts = Rc::new(Cell::new([0; 3]));
    chain
        .register(
            &device,
            Box::new(Counted {
                samples: 4,
                counts: counts.clone(),
            }),
        )
        .unwrap();
    assert_eq!(counts.get(), [0; 3]);
    for size in [[8, 8], [16, 16]] {
        chain.resize(&device, size).unwrap();
        assert_eq!(counts.get(), [0; 3]);
        let resource = ResourceRef::new("color");
        let view = AttachmentView::new(resource, chain.targets().texture()).unwrap();
        let mut pass = PassDeclaration::new(PassSlot::Opaque);
        pass.color = Some(AttachmentDeclaration {
            resource,
            format: config.format,
            size,
            samples: 1,
            load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        });
        let mut encoder = begin(&device, &queue);
        assert_eq!(
            chain.encode_geometry(
                &device,
                &mut encoder,
                &FramePlan {
                    passes: &[pass],
                    views: &[view],
                    requested: &[]
                },
                &mut []
            ),
            Err(FrameError::AttachmentContract(PassSlot::Opaque))
        );
        assert_eq!(counts.get(), [0; 3]);
    }
}

#[test]
fn checked_nonzero_subresources_are_valid_in_generic_executor() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let config = FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1);
    let mut chain = PostChain::new(
        &device,
        config,
        Resources::default(),
        CopyOutput::new(&device, config.format),
    )
    .unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 2,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let resource = ResourceRef {
        mip: 1,
        layer: 1,
        ..ResourceRef::new("color")
    };
    let view = AttachmentView::new(resource, &texture).unwrap();
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(AttachmentDeclaration {
        resource,
        format: config.format,
        size: [8, 8],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::GREEN),
        store: wgpu::StoreOp::Store,
    });
    let mut encoder = begin(&device, &queue);
    chain
        .encode_geometry(
            &device,
            &mut encoder,
            &FramePlan {
                passes: &[pass],
                views: &[view],
                requested: &[],
            },
            &mut [],
        )
        .unwrap();
    encoder.finish().submit().unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn begin(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> worldrunner_render_gpu::submission::RecordingFrame {
    worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(device)
        .unwrap()
}
