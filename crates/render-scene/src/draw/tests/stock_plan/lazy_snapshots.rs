//! A caller feature that reads the opaque colour snapshot on a renderer that was configured
//! without screen-space reflections, UI depth or water. The renderer allocates the snapshot
//! banks while it plans the first frame, so the allocations the first frame was bound to are
//! not the ones the following frames meet unless the plan is stored for the banks it uses.
use super::*;
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_post::{
    AttachmentLoad, BorrowedFeature, DrawContext, FeatureDeclaration, FrameError, PassDeclaration,
    PassSlot, PrepareContext, PreparedFeature,
};

const EXTENT: u32 = 32;

const PAINT: &str = "
@group(0) @binding(0) var<uniform> tint: vec4<f32>;
@vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return tint; }
";

/// Fills the opaque colour with a tint the test sets before each frame, so what a frame wrote
/// to the bank says which frame wrote it while the authored passes stay the same.
struct Painter {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

impl Painter {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> (Self, wgpu::Buffer) {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lazy snapshot paint"),
            source: wgpu::ShaderSource::Wgsl(PAINT.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lazy snapshot paint"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let tint = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lazy snapshot tint"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lazy snapshot paint"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: tint.as_entire_binding(),
            }],
        });
        (Self { pipeline, group }, tint)
    }
}

impl PreparedFeature for Painter {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, _: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Records the texture the frame binds the opaque colour snapshot to.
struct Sampler(Rc<RefCell<Option<wgpu::Texture>>>);

impl PreparedFeature for Sampler {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        let snapshot = context.resource(ResourceRef::new("opaque snapshot color"))?;
        *self.0.borrow_mut() = Some(snapshot.texture().clone());
        Ok(())
    }

    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}

fn caller_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("caller attachment"),
        size: wgpu::Extent3d {
            width: EXTENT,
            height: EXTENT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

/// The half-float texels of a frame painted with a tint whose first channel is `red`.
fn texels(red: u16) -> Vec<u8> {
    (0..EXTENT * EXTENT)
        .flat_map(|_| [red, 0x3000, 0x2c00, 0x3c00])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// The authored passes, which are the same on every frame.
fn passes(color: &AttachmentView, depth: &AttachmentView) -> [PassDeclaration; 2] {
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(crate::compositor::attachment(
        color.resource(),
        color,
        false,
    ));
    opaque.depth = Some(crate::compositor::attachment(depth.resource(), depth, true));
    let mut output = opaque.clone();
    output.slot = PassSlot::ScreenSpaceAfterTransparent;
    for attachment in output.color.iter_mut().chain(&mut output.depth) {
        attachment.load = AttachmentLoad::Load;
    }
    [opaque, output]
}

fn paints() -> FeatureDeclaration {
    FeatureDeclaration::new(PassSlot::Opaque)
}

fn reads_snapshot() -> FeatureDeclaration {
    let mut feature = FeatureDeclaration::new(PassSlot::ScreenSpaceAfterTransparent);
    feature.passes[0].reads = vec![ResourceRef::new("opaque snapshot color")];
    feature
}

fn caller_views(device: &wgpu::Device, format: wgpu::TextureFormat) -> [AttachmentView; 2] {
    let color_texture = caller_texture(
        device,
        format,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
    );
    let depth_texture = caller_texture(
        device,
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    [
        AttachmentView::new(ResourceRef::new("scene color"), &color_texture).unwrap(),
        AttachmentView::new(ResourceRef::new("scene depth"), &depth_texture).unwrap(),
    ]
}

#[test]
fn gpu_caller_snapshot_reader_without_screen_space_effects_hits_from_the_second_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let mut renderer = crate::compositor::FrameRenderer::hdr(
        &context,
        [EXTENT, EXTENT],
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        None,
    )
    .unwrap();
    assert!(
        renderer.snapshots.is_none(),
        "nothing the renderer was configured with allocates the banks"
    );
    let format = renderer.targets().config().format;
    let views = caller_views(device, format);
    let passes = passes(&views[0], &views[1]);
    let view = SceneView::new(
        viewpoint(0),
        camera(),
        Viewport {
            width: EXTENT,
            height: EXTENT,
        },
    )
    .unwrap();
    let frame = FrameUniform::new(&view, None).unwrap();
    let mut readback = Readback::new(
        device,
        FrameConfig::new([EXTENT, EXTENT], format, 1),
        1_000_000,
    )
    .unwrap();

    let (mut painter, tint) = Painter::new(device, format);
    let mut built = None;
    let frames = [
        (0.125_f32, 0x3000_u16),
        (0.25, 0x3400),
        (0.5, 0x3800),
        (0.75, 0x3a00),
    ];
    for (index, (red, bits)) in frames.into_iter().enumerate() {
        let tint_bytes: Vec<u8> = [red, 0.125, 0.0625, 1.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        context.queue().write_buffer(&tint, 0, &tint_bytes);
        let bound = Rc::new(RefCell::new(None));
        let mut sampler = Sampler(bound.clone());
        let mut encoder = context.submission().begin(device).unwrap();
        renderer
            .encode_frame(
                device,
                &mut encoder,
                (&passes, &views),
                None,
                &mut [
                    BorrowedFeature {
                        declaration: paints(),
                        prepared: &mut painter,
                    },
                    BorrowedFeature {
                        declaration: reads_snapshot(),
                        prepared: &mut sampler,
                    },
                ],
                &(frame, None),
            )
            .unwrap();
        encoder.finish().submit().unwrap();

        let frame_number = index + 1;
        let bank = renderer
            .snapshots
            .as_ref()
            .expect("the first frame allocated the banks")[0]
            .color
            .clone();
        let texture = bound.borrow().clone().expect("the caller prepared");
        assert_eq!(texture, bank, "frame {frame_number} samples the bank");
        assert_eq!(
            readback
                .read(device, context.submission(), &texture)
                .unwrap(),
            texels(bits),
            "frame {frame_number} samples the colour it wrote this frame"
        );

        let statistics = renderer.stock_plan_statistics();
        assert_eq!(statistics.rebuilds, 1, "frame {frame_number}");
        assert_eq!(
            statistics.binding_resolutions, 1,
            "frame {frame_number} finds the table the first frame resolved"
        );
        assert_eq!(statistics.reuses, index as u64, "frame {frame_number}");
        let first = *built.get_or_insert(statistics.binding);
        assert_eq!(statistics.binding, first, "frame {frame_number}");
    }
}
