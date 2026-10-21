use super::*;

fn receiver_pipeline(device: &wgpu::Device, abi: ReceiverDescriptor<'_>) -> wgpu::RenderPipeline {
    let (texture_type, sample, slots) = match abi.dimension {
        ReceiverDimension::D2 => (
            "texture_depth_2d",
            "textureSampleCompareLevel(map, cmp, vec2(0.5), 0.25)",
            5,
        ),
        ReceiverDimension::D2Array => (
            "texture_depth_2d_array",
            "textureSampleCompareLevel(map, cmp, vec2(0.5), 3, 0.25)",
            20,
        ),
    };
    receiver_pipeline_with_sample(device, abi, texture_type, sample, slots)
}

pub(super) fn receiver_pipeline_with_sample(
    device: &wgpu::Device,
    abi: ReceiverDescriptor<'_>,
    texture_type: &str,
    sample: &str,
    slots: u32,
) -> wgpu::RenderPipeline {
    let source = format!(
        r"
{TRIANGLE}
{WGSL}
@group(0) @binding({texture}) var map: {texture_type};
@group(0) @binding({sampler}) var cmp: sampler_comparison;
struct Params {{ data: array<vec4<f32>, {slots}> }}
@group(0) @binding({uniform}) var<uniform> params: Params;
@fragment fn fs() -> @builtin(frag_depth) f32 {{
    return {sample} * params.data[{last}].x;
}}
",
        texture = abi.texture_binding,
        sampler = abi.sampler_binding,
        uniform = abi.uniform_binding,
        last = slots - 1
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("receiver ABI probe"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let receiver = receiver_layout(device, abi);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&receiver)],
        immediate_size: 0,
    });
    create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            fragment: Some(DepthFragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            compare: wgpu::CompareFunction::Always,
            bias: wgpu::DepthBiasState::default(),
        },
    )
}

pub(super) fn receiver_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    map: &DepthMap,
    abi: ReceiverDescriptor<'_>,
    compare: wgpu::CompareFunction,
) -> wgpu::BindGroup {
    let size = if abi.dimension == ReceiverDimension::D2 {
        80
    } else {
        320
    };
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, size - 16, &1.0_f32.to_le_bytes());
    let sampler = comparison_sampler(device, compare);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &receiver_layout(device, abi),
        entries: &[
            wgpu::BindGroupEntry {
                binding: abi.texture_binding,
                resource: wgpu::BindingResource::TextureView(map.receiver_view()),
            },
            wgpu::BindGroupEntry {
                binding: abi.sampler_binding,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: abi.uniform_binding,
                resource: buffer.as_entire_binding(),
            },
        ],
    })
}

#[test]
fn receiver_abis_keep_binding_order_uniform_extent_dimension_and_equality_rule() {
    let (device, queue) = device();
    let caster = triangle_pipeline(&device);
    for (abi, compare, layers) in variants() {
        let map = DepthMap::new(&device, descriptor(layers, abi.dimension)).unwrap();
        let output = DepthMap::new(&device, descriptor(1, ReceiverDimension::D2)).unwrap();
        let pipeline = receiver_pipeline(&device, abi);
        let group = receiver_group(&device, &queue, &map, abi, compare);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        clear_layers(&map, &mut encoder);
        {
            let mut pass = map
                .begin_layer_pass(&mut encoder, layers - 1, None)
                .unwrap();
            pass.set_pipeline(&caster);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = output.begin_layer_pass(&mut encoder, 0, None).unwrap();
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        let copy = output
            .copy_layer_to_readback(&device, &mut encoder, 0)
            .unwrap();
        queue.submit([encoder.finish()]);
        let actual = depths(&device, &copy)[8 * 17 + 8];
        let expected = if compare == wgpu::CompareFunction::Less {
            0.0_f32
        } else {
            1.0_f32
        };
        assert!(
            (actual - expected).abs() < 0.00001,
            "{abi:?}: {actual} != {expected}"
        );
    }
}

pub(super) fn variants() -> [(ReceiverDescriptor<'static>, wgpu::CompareFunction, u32); 2] {
    [
        (
            ReceiverDescriptor {
                label: None,
                dimension: ReceiverDimension::D2Array,
                texture_binding: 0,
                sampler_binding: 1,
                uniform_binding: 2,
                min_uniform_binding_size: std::num::NonZeroU64::new(320),
            },
            wgpu::CompareFunction::Less,
            4,
        ),
        (
            ReceiverDescriptor {
                label: None,
                dimension: ReceiverDimension::D2,
                texture_binding: 1,
                sampler_binding: 2,
                uniform_binding: 0,
                min_uniform_binding_size: None,
            },
            wgpu::CompareFunction::LessEqual,
            1,
        ),
    ]
}
