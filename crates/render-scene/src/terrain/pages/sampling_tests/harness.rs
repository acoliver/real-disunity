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
    sample_with(
        device,
        authority,
        cache,
        leaves,
        points,
        Mode {
            work,
            normals: false,
            level: 1,
            baseline: false,
            extent: [1.0; 2],
        },
    )
}

#[derive(Clone, Copy)]
pub(in crate::terrain::pages) struct Mode {
    pub work: bool,
    pub normals: bool,
    pub level: u16,
    pub baseline: bool,
    pub extent: [f32; 2],
}

pub(in crate::terrain::pages) fn sample_with(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    cache: &ResidentPages,
    leaves: &[PageKey],
    points: &[[f32; 4]],
    mode: Mode,
) -> Vec<u8> {
    let table = table(device, cache, leaves, mode);
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
        size: points.len() as u64 * if mode.normals { 16 } else { 4 },
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sampling read"),
        size: output.size(),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let (pipeline, group, empty) = pipeline(device, cache, &table, &input, &output, mode);
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
    mode: Mode,
) -> (wgpu::ComputePipeline, wgpu::BindGroup, wgpu::BindGroup) {
    let source = shader(mode);
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

fn table(
    device: &wgpu::Device,
    cache: &ResidentPages,
    leaves: &[PageKey],
    mode: Mode,
) -> wgpu::Buffer {
    let residents = cache.residents();
    let settings = cache.settings();
    let mut words = vec![
        u32::try_from(leaves.len()).unwrap(),
        u32::try_from(residents.len()).unwrap(),
        cache.product_count() - 2,
        u32::from(mode.level - 1),
        settings.parts().interior[0],
        settings.parts().interior[1],
        settings.parts().border,
        settings.maximum_sampling_mip(),
        0,
        0,
        mode.extent[0].to_bits(),
        mode.extent[1].to_bits(),
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

fn shader(mode: Mode) -> String {
    let source = include_str!("../footprint.wgsl").to_owned()
        + include_str!("../sampling.wgsl")
        + include_str!("kernel.wgsl");
    let source = if mode.baseline {
        source.replace("fn cached_surface(", "fn unused_cached_surface(")
            + include_str!("main_cached_test.wgsl")
    } else {
        source
    };
    let packed = "pack4x8unorm(vec4<f32>(value.albedo, select(0.0, 1.0, finite_surface(value))))";
    if mode.work {
        source.replace(packed, "lookup_page(p.xy, 0.0).work")
    } else if mode.normals {
        source
            .replace("output: array<u32>", "output: array<vec4<f32>>")
            .replace(
                packed,
                "vec4<f32>(value.normal, select(0.0, 1.0, finite_surface(value)))",
            )
    } else {
        source
    }
}
