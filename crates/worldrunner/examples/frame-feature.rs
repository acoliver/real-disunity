//! External feature registration, snapshot sampling and caller-owned submission.
use gpu::{
    executor::{
        AttachmentDeclaration, AttachmentLoad, AttachmentView, BuildContext, DrawContext,
        FeatureDeclaration, FeaturePass, FrameConfig, FrameError, FrameFeature, PassDeclaration,
        PassSlot, PrepareContext, PreparedFeature, Readback, ResourceRef, ToneMap,
    },
    wgpu,
};
use worldrunner::scene::{gpu, plan::SceneCamera};

struct PaintAndRead;
impl FrameFeature for PaintAndRead {
    fn declare(&self) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        let mut reader = FeaturePass::new(PassSlot::TransparentWater);
        reader.reads = vec![
            ResourceRef::new("opaque snapshot color"),
            ResourceRef::new("opaque snapshot depth"),
        ];
        declaration.passes.push(reader);
        for pass in &mut declaration.passes {
            pass.color_format = Some(wgpu::TextureFormat::Rgba16Float);
            pass.depth_format = Some(wgpu::TextureFormat::Depth32Float);
        }
        declaration
    }
    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Prepared {
            paint: pipeline(context.device, "paint", false),
            read: pipeline(context.device, "read", true),
            group: None,
        }))
    }
}
struct Prepared {
    paint: wgpu::RenderPipeline,
    read: wgpu::RenderPipeline,
    group: Option<wgpu::BindGroup>,
}
impl PreparedFeature for Prepared {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let snapshot = context.resource(ResourceRef::new("opaque snapshot color"))?;
        self.group = Some(
            context
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("external snapshot reader"),
                    layout: &self.read.get_bind_group_layout(0),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(snapshot.view()),
                    }],
                }),
        );
        Ok(())
    }
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        if context.slot == PassSlot::Opaque {
            pass.set_pipeline(&self.paint);
        } else {
            pass.set_pipeline(&self.read);
            pass.set_bind_group(0, self.group.as_ref().unwrap(), &[]);
        }
        pass.draw(0..3, 0..1);
    }
}
fn pipeline(device: &wgpu::Device, entry: &str, reader: bool) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("external feature"),
        source: wgpu::ShaderSource::Wgsl(
            r"
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(p[index], 0.5, 1.0);
}
@fragment fn paint() -> @location(0) vec4<f32> { return vec4(0.75, 0.0, 0.0, 1.0); }
@group(0) @binding(0) var snapshot: texture_2d<f32>;
@fragment fn read(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let sampled = textureLoad(snapshot, vec2<i32>(p.xy), 0);
    return vec4(0.0, sampled.r, 0.0, sampled.a);
}"
            .into(),
        ),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("external feature"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(!reader),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
    })
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = pollster::block_on(gpu::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))?;
    let device = context.device();
    let mut renderer = gpu::compositor::FrameRenderer::hdr(
        &context,
        [8, 8],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        None,
    )?;
    renderer.register(device, Box::new(PaintAndRead))?;
    let view = fixture_view()?;
    let frame = gpu::frame::FrameUniform::new(&view, None)?;
    let (passes, views) = frame_plan(&renderer)?;
    let mut target = gpu::offscreen::OffscreenTarget::new(device, view.viewport())?;
    let mut encoder = context.submission().begin(device)?;
    renderer.encode_frame(
        device,
        &mut encoder,
        (&passes, &views),
        &mut [],
        (frame, None),
    )?;
    renderer.encode_output(&mut encoder, target.color_view())?;
    target.encode_readback(&mut encoder)?;
    let submitted = encoder.finish().submit()?;
    let image = target.read_encoded(&context, submitted)?;
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .all(|pixel| pixel[0] == 0 && pixel[1] > 0 && pixel[2..] == [0, 255])
    );
    let (snapshot, _) = renderer
        .opaque_snapshot()
        .expect("declared snapshot inputs");
    let bytes = Readback::new(
        device,
        FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba16Float, 1),
        4096,
    )?
    .read(device, context.submission(), snapshot.texture())?;
    assert!(
        bytes
            .chunks_exact(8)
            .all(|pixel| pixel == [0, 58, 0, 0, 0, 0, 0, 60])
    );
    Ok(())
}

fn fixture_view() -> Result<gpu::SceneView, gpu::ViewError> {
    gpu::SceneView::new(
        gpu::CameraPose {
            eye_millimeters: [0, -2000, 2000],
            target_millimeters: [0; 3],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        gpu::Viewport {
            width: 8,
            height: 8,
        },
    )
}

fn frame_plan(
    renderer: &gpu::compositor::FrameRenderer,
) -> Result<([PassDeclaration; 2], [AttachmentView; 2]), FrameError> {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )?;
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )?;
    let attachment = |view: &AttachmentView, load| AttachmentDeclaration {
        resource: view.resource(),
        format: view.format(),
        size: view.size(),
        samples: 1,
        load,
        store: wgpu::StoreOp::Store,
    };
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(attachment(
        &color,
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
    ));
    opaque.depth = Some(attachment(&depth, AttachmentLoad::ClearDepth(1.0)));
    let mut transparent = opaque.clone();
    transparent.slot = PassSlot::TransparentWater;
    transparent.color.as_mut().unwrap().load = AttachmentLoad::Load;
    transparent.depth.as_mut().unwrap().load = AttachmentLoad::Load;
    Ok(([opaque, transparent], [color, depth]))
}
