use super::{GameScreenComposite, attachment};
use crate::device::GpuContext;
use std::cell::Cell;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, DrawContext, FeatureDeclaration, FeaturePass, FrameConfig,
    FrameError, FrameFeature, FrameOperations, FramePlan, OperationsGroup, OperationsOutput,
    PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, Readback, ResourceRef,
    color_texture,
};

const SIZE: u32 = 16;

const SCREEN: ResourceRef = ResourceRef::new("game-screen");
const ENTRY: PassEntry = PassEntry::new(PassSlot::Opaque);
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Rig {
    context: std::rc::Rc<GpuContext>,
    renderer: super::FrameRenderer,
}

impl Rig {
    fn new() -> Self {
        let context = std::rc::Rc::new(GpuContext::shared_for_test().unwrap());
        let mut renderer = super::FrameRenderer::with_output(
            context.device(),
            FrameConfig::new([SIZE, SIZE], wgpu::TextureFormat::Rgba16Float, 1),
            OUTPUT,
        )
        .unwrap();
        let format = renderer.targets().scene().texture().format();
        renderer
            .register(
                context.device(),
                Box::new(GameScreenComposite::new(SCREEN, ENTRY, format)),
            )
            .unwrap();
        Self { context, renderer }
    }

    fn format(&self) -> wgpu::TextureFormat {
        self.renderer.targets().scene().texture().format()
    }
}

fn screen_colour() -> wgpu::Color {
    let linear = [128.0, 64.0, 32.0].map(|byte: f64| ((byte / 255.0 + 0.055) / 1.055).powf(2.4));
    wgpu::Color {
        r: linear[0],
        g: linear[1],
        b: linear[2],
        a: 1.0,
    }
}

struct ScreenWrite {
    view: wgpu::TextureView,
    color: wgpu::Color,
    calls: Cell<u32>,
}

impl ScreenWrite {
    fn new(screen: &wgpu::Texture, color: wgpu::Color) -> Self {
        Self {
            view: screen.create_view(&wgpu::TextureViewDescriptor::default()),
            color,
            calls: Cell::new(0),
        }
    }
    fn before_composite(&self) -> OperationsGroup<'_> {
        OperationsGroup {
            before: Some(ENTRY),
            work: self,
        }
    }
}

impl FrameOperations for ScreenWrite {
    fn produces(&self) -> &[OperationsOutput] {
        static OUTPUTS: [OperationsOutput; 1] =
            [OperationsOutput::new(SCREEN, OUTPUT, [SIZE, SIZE])];
        &OUTPUTS
    }

    fn encode_operations(
        &self,
        _: Option<&worldrunner_render_post::TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        self.calls.set(self.calls.get() + 1);
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("synthetic game screen"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        Ok(())
    }
}

struct Overlay(wgpu::RenderPipeline);

impl PreparedFeature for Overlay {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_scissor_rect(0, 0, 2, 2);
        pass.set_pipeline(&self.0);
        pass.draw(0..3, 0..1);
    }
}

fn overlay(device: &wgpu::Device, format: wgpu::TextureFormat) -> Overlay {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("synthetic overlay"),
        source: wgpu::ShaderSource::Wgsl(
            "@vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));return vec4<f32>(p[i],0.0,1.0);}@fragment fn fragment()->@location(0) vec4<f32>{return vec4<f32>(1.0,0.0,0.0,1.0);}".into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("synthetic overlay"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let pipeline = worldrunner_render_post::fullscreen_pipeline(
        device,
        worldrunner_render_post::FullscreenDescriptor {
            label: Some("synthetic overlay"),
            layout: &layout,
            module: &shader,
            vertex_entry: "vertex",
            fragment_entry: "fragment",
            format,
            constants: &[],
        },
    );
    Overlay(pipeline)
}

fn scene_pass(rig: &Rig) -> (PassDeclaration, AttachmentView) {
    let texture = rig.renderer.targets().scene().texture();
    let view = AttachmentView::new(ResourceRef::new("scene color"), texture).unwrap();
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(view.resource(), &view, false));
    (pass, view)
}

fn output_texture(device: &wgpu::Device) -> wgpu::Texture {
    color_texture(device, "composite output", [SIZE, SIZE], OUTPUT)
}

fn pixels(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([SIZE, SIZE], texture.format(), 1),
        1 << 20,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

fn pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * SIZE + x) * 4) as usize;
    bytes[offset..offset + 4].try_into().unwrap()
}

/// Read the linear target before output transfer. Allow only the interval that
/// round-trips to each source byte after binary16 storage and sRGB encoding.
fn assert_linear_centre(context: &GpuContext, target: &wgpu::Texture) {
    assert_eq!(target.format(), wgpu::TextureFormat::Rgba16Float);
    let bytes = pixels(context, target);
    let offset = ((SIZE / 2 * SIZE + SIZE / 2) * 8) as usize;
    let channels = [0, 2, 4, 6]
        .map(|n| half::f16::from_le_bytes([bytes[offset + n], bytes[offset + n + 1]]).to_f64());
    for (actual, code) in channels[..3].iter().zip([128.0, 64.0, 32.0]) {
        let decode = |byte: f64| ((byte / 255.0 + 0.055) / 1.055).powf(2.4);
        assert!(
            (decode(code - 0.5)..decode(code + 0.5)).contains(actual),
            "sRGB byte {code} decoded to {actual}"
        );
    }
    assert_eq!(channels[3].to_bits(), 1.0_f64.to_bits());
}

fn submit_frame(
    rig: &mut Rig,
    context: &GpuContext,
    plan: &FramePlan<'_>,
    features: &mut [BorrowedFeature<'_>],
    output: &wgpu::TextureView,
) -> Result<(), FrameError> {
    let device = context.device();
    let serial = context.submission().submitted_serial();
    let transaction = context
        .submission()
        .prepare(device, |encoder, _| {
            rig.renderer
                .encode_frozen(device, encoder, plan, features)?;
            rig.renderer.encode_output(encoder, output).unwrap();
            Ok::<_, FrameError>(())
        })
        .map_err(|error| match error {
            worldrunner_render_gpu::submission::PrepareError::Preparation(error) => error,
            other => panic!("the frame transaction did not start: {other:?}"),
        })?;
    assert_eq!(
        context.submission().submitted_serial(),
        serial,
        "encoding must not submit"
    );
    transaction.submit().unwrap();
    assert_eq!(context.submission().submitted_serial(), serial + 1);
    Ok(())
}

#[test]
fn multisampled_composite_targets_are_refused_by_the_frame_contract() {
    let feature = GameScreenComposite::new(SCREEN, ENTRY, wgpu::TextureFormat::Rgba16Float);
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(worldrunner_render_post::AttachmentDeclaration {
        resource: ResourceRef::new("target"),
        format: wgpu::TextureFormat::Rgba16Float,
        size: [SIZE, SIZE],
        samples: 4,
        load: worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    assert_eq!(
        worldrunner_render_post::validate_frame(&[pass], &[feature.declare()], &[]),
        Err(FrameError::AttachmentContract(ENTRY))
    );
}

#[test]
fn current_operation_screen_is_composited_and_later_overlay_wins() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let context = rig.context.clone();
    let device = context.device();
    let screen = color_texture(device, "game screen", [SIZE, SIZE], OUTPUT);
    let source = AttachmentView::new(SCREEN, &screen)
        .unwrap()
        .with_retained_contents();
    let writer = ScreenWrite::new(&screen, screen_colour());
    let group = writer.before_composite();
    let (pass, target) = scene_pass(&rig);
    let mut overlay_pass = pass.clone();
    overlay_pass.occurrence = 1;
    overlay_pass.color.as_mut().unwrap().load = worldrunner_render_post::AttachmentLoad::Load;
    let mut overlay_contract = FeaturePass::nth(PassSlot::Opaque, 1);
    overlay_contract.color_format = Some(rig.format());
    let mut draw = overlay(device, rig.format());
    let mut borrowed = [BorrowedFeature {
        declaration: FeatureDeclaration {
            passes: vec![overlay_contract],
            effects: Vec::new(),
        },
        prepared: &mut draw,
    }];
    let passes = [pass, overlay_pass];
    let views = [target, source];
    let output = output_texture(device);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let before = context.submission().submitted_serial();
    let plan = FramePlan {
        passes: &passes,
        views: &views,
        requested: &[],
        operations: &[group],
    };
    submit_frame(&mut rig, &context, &plan, &mut borrowed, &output_view).unwrap();
    assert_eq!(context.submission().submitted_serial(), before + 1);
    assert_eq!(writer.calls.get(), 1);
    assert_linear_centre(&context, rig.renderer.targets().scene().texture());
    let image = pixels(&context, &output);
    assert_eq!(pixel(&image, 1, 1), [255, 0, 0, 255]);
    let center = pixel(&image, SIZE / 2, SIZE / 2);
    assert_eq!(center, [128, 64, 32, 255]);
}

#[test]
fn bottom_first_srgba_screen_scales_with_top_first_output() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let context = rig.context.clone();
    let device = context.device();
    let screen = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("corner screen"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OUTPUT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue().write_texture(
        screen.as_image_copy(),
        &[
            128, 0, 0, 255, 0, 64, 0, 255, 0, 0, 32, 255, 128, 64, 32, 255,
        ],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        screen.size(),
    );
    let source = AttachmentView::new(SCREEN, &screen)
        .unwrap()
        .with_retained_contents();
    let (pass, target) = scene_pass(&rig);
    let views = [target, source];
    let output = output_texture(device);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let plan = FramePlan {
        passes: std::slice::from_ref(&pass),
        views: &views,
        requested: &[],
        operations: &[],
    };
    submit_frame(&mut rig, &context, &plan, &mut [], &output_view).unwrap();
    let image = pixels(&context, &output);
    assert_eq!(pixel(&image, 2, 2), [0, 0, 32, 255]);
    assert_eq!(pixel(&image, 13, 2), [128, 64, 32, 255]);
    assert_eq!(pixel(&image, 2, 13), [128, 0, 0, 255]);
    assert_eq!(pixel(&image, 13, 13), [0, 64, 0, 255]);
}

fn cancel_frame(
    rig: &mut Rig,
    context: &GpuContext,
    plan: &FramePlan<'_>,
    output: &wgpu::TextureView,
) {
    let device = context.device();
    let serial = context.submission().submitted_serial();
    let frame = context
        .submission()
        .prepare(device, |encoder, _| {
            rig.renderer.encode_frozen(device, encoder, plan, &mut [])?;
            rig.renderer.encode_output(encoder, output).unwrap();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    frame.cancel();
    assert_eq!(context.submission().submitted_serial(), serial);
}

#[test]
fn retained_screen_composites_without_a_current_frame_writer() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let context = rig.context.clone();
    let device = context.device();
    let screen = color_texture(device, "retained game screen", [SIZE, SIZE], OUTPUT);
    let source = AttachmentView::new(SCREEN, &screen)
        .unwrap()
        .with_retained_contents();
    let writer = ScreenWrite::new(&screen, screen_colour());
    let group = writer.before_composite();
    let (pass, target) = scene_pass(&rig);
    let views = [target, source];
    let output = output_texture(device);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let plan = FramePlan {
        passes: std::slice::from_ref(&pass),
        views: &views,
        requested: &[],
        operations: &[group],
    };
    submit_frame(&mut rig, &context, &plan, &mut [], &output_view).unwrap();
    let first = pixels(&context, &output);
    let abandoned = ScreenWrite::new(&screen, wgpu::Color::BLACK);
    cancel_frame(
        &mut rig,
        &context,
        &FramePlan {
            passes: std::slice::from_ref(&pass),
            views: &views,
            requested: &[],
            operations: &[abandoned.before_composite()],
        },
        &output_view,
    );

    let plan = FramePlan {
        passes: std::slice::from_ref(&pass),
        views: &views,
        requested: &[],
        operations: &[],
    };
    submit_frame(&mut rig, &context, &plan, &mut [], &output_view).unwrap();
    assert_eq!(writer.calls.get(), 1);
    let image = pixels(&context, &output);
    assert_eq!(
        abandoned.calls.get(),
        1,
        "the cancelled encoder recorded the writer"
    );
    assert_eq!(
        image, first,
        "cancellation did not replace retained screen contents"
    );
    let center = pixel(&image, SIZE / 2, SIZE / 2);
    assert_eq!(center, [128, 64, 32, 255]);
}

#[test]
fn missing_or_late_screen_writers_are_refused_before_encoding() {
    let _session = crate::test_support::SceneTestSession::acquire();
    for late in [false, true] {
        let mut rig = Rig::new();
        let context = rig.context.clone();
        let device = context.device();
        let screen = color_texture(device, "unwritten screen", [SIZE, SIZE], OUTPUT);
        let source = AttachmentView::new(SCREEN, &screen).unwrap();
        let source = if late {
            source.with_retained_contents()
        } else {
            source
        };
        let writer = ScreenWrite::new(&screen, wgpu::Color::WHITE);
        let groups = if late {
            vec![OperationsGroup {
                before: None,
                work: &writer,
            }]
        } else {
            vec![]
        };
        let (pass, target) = scene_pass(&rig);
        let output = output_texture(device).create_view(&wgpu::TextureViewDescriptor::default());
        let before = context.submission().submitted_serial();
        let result = submit_frame(
            &mut rig,
            &context,
            &FramePlan {
                passes: &[pass],
                views: &[target, source],
                requested: &[],
                operations: &groups,
            },
            &mut [],
            &output,
        );
        assert_eq!(result, Err(FrameError::MissingProducer(SCREEN)));
        assert_eq!(writer.calls.get(), 0);
        assert_eq!(context.submission().submitted_serial(), before);
    }
}

#[test]
fn a_discarded_screen_is_refused_before_compositing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let context = rig.context.clone();
    let device = context.device();
    let screen = color_texture(device, "discarded screen", [SIZE, SIZE], OUTPUT);
    let source = AttachmentView::new(SCREEN, &screen).unwrap();
    let mut producer = PassDeclaration::new(PassSlot::DepthNormalPrepass);
    let mut color = attachment(SCREEN, &source, false);
    color.store = wgpu::StoreOp::Discard;
    producer.color = Some(color);
    let (pass, target) = scene_pass(&rig);
    let output = output_texture(device).create_view(&wgpu::TextureViewDescriptor::default());
    let before = context.submission().submitted_serial();
    let result = submit_frame(
        &mut rig,
        &context,
        &FramePlan {
            passes: &[producer, pass],
            views: &[target, source],
            requested: &[],
            operations: &[],
        },
        &mut [],
        &output,
    );
    assert_eq!(result, Err(FrameError::LoadAfterDiscard(SCREEN)));
    assert_eq!(context.submission().submitted_serial(), before);
}

#[test]
fn depth_attached_composite_targets_are_refused_before_resource_lookup() {
    let feature = GameScreenComposite::new(SCREEN, ENTRY, wgpu::TextureFormat::Rgba16Float);
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    let color = worldrunner_render_post::AttachmentDeclaration {
        resource: ResourceRef::new("target"),
        format: wgpu::TextureFormat::Rgba16Float,
        size: [SIZE, SIZE],
        samples: 1,
        load: worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    };
    pass.color = Some(color.clone());
    pass.depth = Some(worldrunner_render_post::AttachmentDeclaration {
        resource: ResourceRef::new("depth"),
        format: wgpu::TextureFormat::Depth32Float,
        load: worldrunner_render_post::AttachmentLoad::ClearDepth(1.0),
        ..color
    });
    assert_eq!(
        worldrunner_render_post::validate_frame(&[pass], &[feature.declare()], &[]),
        Err(FrameError::AttachmentContract(ENTRY)),
    );
}

#[test]
fn replacing_the_screen_allocation_rebinds_the_named_source() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let context = rig.context.clone();
    let device = context.device();
    let output = output_texture(device);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let (pass, target) = scene_pass(&rig);
    for (colour, expected) in [
        (screen_colour(), [128, 64, 32, 255]),
        (wgpu::Color::WHITE, [255; 4]),
    ] {
        let screen = color_texture(device, "replacement screen", [SIZE, SIZE], OUTPUT);
        let source = AttachmentView::new(SCREEN, &screen)
            .unwrap()
            .with_retained_contents();
        let writer = ScreenWrite::new(&screen, colour);
        submit_frame(
            &mut rig,
            &context,
            &FramePlan {
                passes: std::slice::from_ref(&pass),
                views: &[target.clone(), source],
                requested: &[],
                operations: &[writer.before_composite()],
            },
            &mut [],
            &output_view,
        )
        .unwrap();
        assert_eq!(writer.calls.get(), 1);
        assert_eq!(
            pixel(&pixels(&context, &output), SIZE / 2, SIZE / 2),
            expected
        );
    }
}
