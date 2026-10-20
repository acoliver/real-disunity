use std::{sync::mpsc, time::Duration};
use worldrunner_render_gpu::{device::*, resource::*, texture::*};

#[test]
fn explicit_material_bindings_reach_shader_without_renumbering() {
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("material regression requires a GPU adapter");
    let (device, queue, _) =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default())).unwrap();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("explicit material contract"),
        source: wgpu::ShaderSource::Wgsl(include_str!("material.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("explicit material contract"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let prepared = prepare_material(&device, &queue, &pipeline.get_bind_group_layout(0));
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 20,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: output.as_entire_binding(),
        }],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 20,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &prepared.bind_group, &[]);
        pass.set_bind_group(1, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 20);
    queue.submit([encoder.finish()]);
    assert_eq!(read_values(&device, &readback), [111, 122, 133, 144, 155]);
    assert!(pollster::block_on(scope.pop()).is_none());
}

fn prepare_material(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
) -> PreparedMaterial {
    let textures: Vec<_> = [11u8, 22, 33, 44, 55]
        .into_iter()
        .map(|value| {
            let pixel = [value, 0, 0, 255];
            let mips = packed_mips(wgpu::TextureFormat::Rgba8Unorm, 1, 1, 1, &pixel).unwrap();
            TextureUpload::new(
                &[TextureLayer {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    width: 1,
                    height: 1,
                    mips: &mips,
                }],
                0,
                TextureKind::D2,
            )
            .unwrap()
            .upload(
                device,
                queue,
                "material input",
                wgpu::TextureUsages::empty(),
            )
            .unwrap()
            .create_view(&wgpu::TextureViewDescriptor::default())
        })
        .collect();
    let bound: Vec<_> = textures.iter().collect();
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    PreparedMaterial::new(
        device,
        queue,
        MaterialDescriptor {
            label: "explicit material",
            layout,
            uniform_bytes: &[100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            textures: &bound,
            sampler: &sampler,
            bindings: Some(MaterialBindings {
                uniform: 3,
                textures: &[0, 1, 4, 5, 6],
                sampler: 2,
            }),
        },
    )
}

fn read_values(device: &wgpu::Device, readback: &wgpu::Buffer) -> Vec<u32> {
    let (send, recv) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    recv.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let values: Vec<_> = mapped
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    drop(mapped);
    readback.unmap();
    values
}
