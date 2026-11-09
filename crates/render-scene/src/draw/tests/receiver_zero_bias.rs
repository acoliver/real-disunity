//! Fragment-stage arithmetic controls for zero normal bias and rejected quad neighbours.
use super::*;
use crate::scene_metadata::receiver::ReceiverCapture;
use wgpu::util::DeviceExt;

fn operand_source(bias: f32, capture: ReceiverCapture) -> String {
    let (producer, _) = crate::scene_metadata::receiver::producer_sources(
        crate::scene_metadata::MetadataOutput::Merged,
        ReceiverLayout::Front,
        bias,
        capture,
    );
    let capture = &producer[producer.rfind("const SCENE_RECEIVER_SIDES:").unwrap()..];
    format!("{capture}\n{}", include_str!("receiver_zero_bias.wgsl"))
}

fn operands(context: &GpuContext, source: &str, normal: u32) -> Vec<Vec<u8>> {
    let device = context.device();
    let bits: Vec<u32> = (0..16)
        .flat_map(|i| {
            let sign = (i % 2) << 31;
            [
                normal | sign,
                normal | (((i / 4) % 2) << 31),
                0,
                sign ^ (((i / 4) % 2) << 31),
            ]
        })
        .collect();
    let bytes: Vec<u8> = bits.iter().flat_map(|value| value.to_le_bytes()).collect();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("receiver zero bias operands"),
        contents: &bytes,
        usage: BufferUsages::STORAGE,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("receiver zero bias arithmetic"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let textures = operand_targets(device);
    let pipeline = operand_pipeline(device, &shader);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("receiver operand input"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    let views: Vec<_> = textures
        .iter()
        .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
        .collect();
    let attachments: Vec<_> = views
        .iter()
        .map(|view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        })
        .collect();
    let mut recording = context.submission().begin(device).unwrap();
    {
        let mut pass = recording.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &attachments,
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    recording.finish().submit().unwrap();
    textures
        .iter()
        .map(|texture| read_payload(context, texture))
        .collect()
}

fn operand_targets(device: &wgpu::Device) -> Vec<Texture> {
    ReceiverLayout::Front
        .formats()
        .iter()
        .map(|format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("receiver arithmetic bits"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: *format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        })
        .collect()
}

fn operand_pipeline(device: &wgpu::Device, shader: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    let targets: Vec<_> = ReceiverLayout::Front
        .formats()
        .into_iter()
        .map(|format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        })
        .collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("receiver arithmetic"),
        layout: None,
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fragment"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

#[test]
fn gpu_zero_bias_arithmetic_preserves_nonfinite_and_signed_zero_operand_bits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for bias in [0.0_f32, -0.0] {
        for normal in [
            0_u32,
            1,
            f32::MIN_POSITIVE.to_bits(),
            1.0_f32.to_bits(),
            f32::MAX.to_bits(),
            f32::INFINITY.to_bits(),
            f32::NAN.to_bits(),
            0x7fc0_1234,
        ] {
            let reference = operands(
                &context,
                &operand_source(bias, ReceiverCapture::Normal),
                normal,
            );
            let position = operands(
                &context,
                &operand_source(bias, ReceiverCapture::Position),
                normal,
            );
            let differences: usize = reference
                .iter()
                .zip(&position)
                .map(|(a, b)| a.iter().zip(b).filter(|(a, b)| a != b).count())
                .sum();
            println!(
                "ZERO_BIAS_OPERANDS bias={:08x} normal={normal:08x} differing_bytes={differences}",
                bias.to_bits()
            );
            assert_eq!(differences, 0, "bias={bias} normal={normal:08x}");
        }
    }
}

#[test]
fn gpu_zero_bias_arithmetic_control_observes_the_normal_operand_at_nonzero_bias() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let zero = operands(
        &context,
        &operand_source(0.0, ReceiverCapture::Normal),
        1.0_f32.to_bits(),
    );
    let biased = operands(
        &context,
        &operand_source(0.012, ReceiverCapture::Normal),
        1.0_f32.to_bits(),
    );
    assert!(
        zero != biased,
        "the fragment control must observe normal derivatives at nonzero bias"
    );
}
