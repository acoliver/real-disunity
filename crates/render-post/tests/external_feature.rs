use std::{cell::RefCell, rc::Rc};
use worldrunner_render_post::*;

struct Triangle {
    id: u8,
    events: Rc<RefCell<Vec<u8>>>,
}
struct PreparedTriangle {
    id: u8,
    events: Rc<RefCell<Vec<u8>>>,
    pipeline: wgpu::RenderPipeline,
}
impl FrameFeature for Triangle {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0].color_format = Some(wgpu::TextureFormat::Rgba8Unorm);
        declaration
    }
    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        self.events.borrow_mut().push(self.id + 20);
        let module = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("external triangle"),
                source: wgpu::ShaderSource::Wgsl(
                    r"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
override GREEN: f32 = 0.0;
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(1.0 - GREEN, GREEN, 0.0, 1.0); }
"
                    .into(),
                ),
            });
        let layout = context
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[],
                immediate_size: 0,
            });
        let pipeline = fullscreen_pipeline(
            context.device,
            FullscreenDescriptor {
                label: None,
                layout: &layout,
                module: &module,
                vertex_entry: "vs",
                fragment_entry: "fs",
                format: context.targets.config().format,
                constants: &[("GREEN", f64::from(self.id))],
            },
        );
        Ok(Box::new(PreparedTriangle {
            id: self.id,
            events: self.events.clone(),
            pipeline,
        }))
    }
}
impl PreparedFeature for PreparedTriangle {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.events.borrow_mut().push(self.id + 10);
        Ok(())
    }
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.slot, PassSlot::Opaque);
        self.events.borrow_mut().push(self.id);
        pass.set_pipeline(&self.pipeline);
        pass.draw(0..3, 0..1);
    }
}

#[test]
fn external_features_draw_in_registration_order_in_one_frame_submission() {
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
    let events = Rc::new(RefCell::new(Vec::new()));
    let first = chain
        .register(
            &device,
            Box::new(Triangle {
                id: 0,
                events: events.clone(),
            }),
        )
        .unwrap();
    let second = chain
        .register(
            &device,
            Box::new(Triangle {
                id: 1,
                events: events.clone(),
            }),
        )
        .unwrap();
    assert_ne!(first, second);
    let output = color_texture(&device, "external output", config.size, config.format);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(&device)
        .unwrap();
    assert_view_contracts(&device, &mut chain, &mut encoder);
    encode_geometry(&device, &mut chain, &mut encoder, &events);
    chain.encode(&mut encoder, &output_view);
    let mut readback = Readback::new(&device, config, 4096).unwrap();
    readback.encode(&mut encoder, &output).unwrap();
    let submission = encoder.finish().submit().unwrap();
    assert_eq!(*events.borrow(), [20, 21, 19, 10, 11, 9, 0, 1]);
    let pixels = readback.read_encoded(&device, Some(submission)).unwrap();
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255])
    );
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn encode_geometry(
    device: &wgpu::Device,
    chain: &mut PostChain<CopyOutput>,
    encoder: &mut wgpu::CommandEncoder,
    events: &Rc<RefCell<Vec<u8>>>,
) {
    let config = chain.targets().config();
    let resource = ResourceRef::new("opaque");
    let mut declaration = PassDeclaration::new(PassSlot::Opaque);
    declaration.color = Some(AttachmentDeclaration {
        resource,
        format: config.format,
        size: config.size,
        samples: 1,
        load: AttachmentLoad::Load,
        store: wgpu::StoreOp::Store,
    });
    let view = chain.targets().scene().clone();
    let views = [AttachmentView::new(resource, view.texture()).unwrap()];
    assert_eq!(
        chain.encode_geometry(
            device,
            encoder,
            &FramePlan {
                passes: &[declaration.clone()],
                views: &views,
                requested: &[]
            },
            &mut []
        ),
        Err(FrameError::MissingProducer(resource))
    );
    assert!(
        events.borrow().is_empty(),
        "invalid plans cannot prepare or draw"
    );
    declaration.color.as_mut().unwrap().load = AttachmentLoad::ClearColor(wgpu::Color::BLACK);
    let mut borrowed = OrderMarker(events.clone());
    chain
        .encode_geometry(
            device,
            encoder,
            &FramePlan {
                passes: &[declaration],
                views: &views,
                requested: &[],
            },
            &mut [BorrowedFeature {
                declaration: FeatureDeclaration::new(PassSlot::Opaque),
                prepared: &mut borrowed,
            }],
        )
        .unwrap();
}

fn assert_view_contracts(
    device: &wgpu::Device,
    chain: &mut PostChain<CopyOutput>,
    encoder: &mut wgpu::CommandEncoder,
) {
    let resource = ResourceRef::new("opaque");
    let config = chain.targets().config();
    let texture = chain.targets().texture().clone();
    assert!(matches!(
        AttachmentView::new(ResourceRef { mip: 1, ..resource }, &texture),
        Err(FrameError::AttachmentView(_))
    ));
    let alias = ResourceRef::new("alias");
    let views = [
        AttachmentView::new(resource, &texture).unwrap(),
        AttachmentView::new(alias, &texture).unwrap(),
    ];
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(AttachmentDeclaration {
        resource,
        format: config.format,
        size: config.size,
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
    });
    assert_eq!(
        chain.encode_geometry(
            device,
            encoder,
            &FramePlan {
                passes: &[pass.clone()],
                views: &views,
                requested: &[]
            },
            &mut []
        ),
        Err(FrameError::AttachmentHazard(alias))
    );
    assert_eq!(
        chain.encode_geometry(
            device,
            encoder,
            &FramePlan {
                passes: &[pass],
                views: &views[..1],
                requested: &["SSAO"]
            },
            &mut []
        ),
        Err(FrameError::UnsupportedEffect("SSAO"))
    );
}

struct OrderMarker(Rc<RefCell<Vec<u8>>>);
impl PreparedFeature for OrderMarker {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.0.borrow_mut().push(19);
        Ok(())
    }
    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        self.0.borrow_mut().push(9);
    }
}
