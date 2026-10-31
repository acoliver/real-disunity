use super::*;
use glam::Vec3;

#[test]
fn cached_anisotropy_exact_zero_normal_sum_preserves_first_unit_tap() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let taps = [[1.0_f32, 0.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0]];
    let input = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("decoded unit normal taps"),
        size: 32,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: true,
    });
    input
        .slice(..)
        .get_mapped_range_mut()
        .unwrap()
        .copy_from_slice(
            &taps
                .into_iter()
                .flatten()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    input.unmap();
    let output = reduction_buffer(
        device,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let read = reduction_buffer(
        device,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let (pipeline, group) = reduction_pipeline(device, &input, &output);
    let mut frame = authority.begin(device).unwrap();
    {
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    frame.copy_buffer_to_buffer(&output, 0, &read, 0, 32);
    frame.finish().submit().unwrap();
    let bytes = super::super::super::gpu_tests::read(device, &read);
    let values: Vec<_> = bytes
        .chunks_exact(4)
        .map(|value| f32::from_le_bytes(value.try_into().unwrap()))
        .collect();
    assert_eq!(&values[..3], &[0.0; 3], "GPU tap sum must be exactly zero");
    let direction = Vec3::from_slice(&values[4..7]);
    assert!(direction.is_finite(), "direction={direction:?}");
    assert!(
        (direction.length() - 1.0).abs() < 1e-6,
        "direction={direction:?}"
    );
    assert_eq!(
        direction,
        Vec3::X,
        "exact cancellation must preserve the first tap"
    );
}

fn reduction_buffer(device: &wgpu::Device, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("normal reduction readback"),
        size: 32,
        usage,
        mapped_at_creation: false,
    })
}

fn reduction_pipeline(
    device: &wgpu::Device,
    input: &wgpu::Buffer,
    output: &wgpu::Buffer,
) -> (wgpu::ComputePipeline, wgpu::BindGroup) {
    let source = include_str!("../../footprint.wgsl").to_owned()
        + include_str!("../../sampling.wgsl")
        + include_str!("reduction.wgsl");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production cached normal reduction"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("production cached normal reduction"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("normal reduction inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    (pipeline, group)
}
