use super::{PageKey, ResidentPages, SubmissionAuthority};
pub(in crate::terrain::pages) fn sample(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    cache: &ResidentPages,
    leaves: &[PageKey],
    points: &[[f32; 4]],
) -> Vec<u8> {
    sample_metric(device, authority, cache, leaves, points, false)
}

pub(in crate::terrain::pages) fn sample_metric(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    cache: &ResidentPages,
    leaves: &[PageKey],
    points: &[[f32; 4]],
    work: bool,
) -> Vec<u8> {
    let table = table(device, cache, leaves);
    let input = buffer(
        device,
        &points
            .iter()
            .flatten()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sampling output"),
        size: points.len() as u64 * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sampling read"),
        size: output.size(),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let (pipeline, group, empty) = pipeline(device, cache, &table, &input, &output, work);
    let mut frame = authority.begin(device).unwrap();
    {
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &empty, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(u32::try_from(points.len()).unwrap(), 1, 1);
    }
    frame.copy_buffer_to_buffer(&output, 0, &read, 0, output.size());
    frame.finish().submit().unwrap();
    super::super::gpu_tests::read(device, &read)
}
fn buffer(device: &wgpu::Device, bytes: &[u8]) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sampling input"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .unwrap()
        .copy_from_slice(bytes);
    buffer.unmap();
    buffer
}

fn pipeline(
    device: &wgpu::Device,
    cache: &ResidentPages,
    table: &wgpu::Buffer,
    input: &wgpu::Buffer,
    output: &wgpu::Buffer,
    work: bool,
) -> (wgpu::ComputePipeline, wgpu::BindGroup, wgpu::BindGroup) {
    let source = "struct BlendedSurface { albedo: vec3<f32>, normal: vec3<f32>, roughness: f32, metalness: f32 }\n".to_owned() + include_str!("../sampling.wgsl") + "\n@group(1) @binding(8) var<storage, read_write> output: array<u32>;\n@group(1) @binding(9) var<storage, read> points: array<vec4<f32>>;\n@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) { let p = points[id.x]; let value = cached_surface(p.xy, vec2<f32>(p.z, 0.0), vec2<f32>(0.0, p.w)); output[id.x] = pack4x8unorm(vec4<f32>(value.albedo, 1.0)); }";
    let source = if work {
        source.replace(
            "pack4x8unorm(vec4<f32>(value.albedo, 1.0))",
            "lookup_page(p.xy, 0.0).work",
        )
    } else {
        source
    };
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shared terrain sampling test"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("shared terrain sampling test"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let texture = cache.texture_view();
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sampling test"),
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(&texture),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: table.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: input.as_entire_binding(),
            },
        ],
    });
    let empty = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[],
    });

    (pipeline, group, empty)
}

fn table(device: &wgpu::Device, cache: &ResidentPages, leaves: &[PageKey]) -> wgpu::Buffer {
    let residents = cache.residents();
    let settings = cache.settings();
    let mut words = vec![
        u32::try_from(leaves.len()).unwrap(),
        u32::try_from(residents.len()).unwrap(),
        0,
        0,
        settings.parts().interior[0],
        settings.parts().interior[1],
        settings.parts().border,
        settings.maximum_sampling_mip(),
        0,
        0,
        1.0f32.to_bits(),
        1.0f32.to_bits(),
    ];
    for record in super::super::scene::table::records(leaves, &residents) {
        words.extend(record);
    }
    buffer(
        device,
        &words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}
