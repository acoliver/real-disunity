use super::*;

mod fit;
mod receiver;
mod sampling;
mod validation;

fn descriptor(layers: u32, dimension: ReceiverDimension) -> DepthMapDescriptor<'static> {
    DepthMapDescriptor {
        label: Some("procedural test depth"),
        resolution: 17,
        layers,
        receiver_dimension: dimension,
        readback: true,
        budget: DepthMapBudget {
            max_resolution: 64,
            max_layers: 4,
            max_texture_bytes: 64 * 64 * 4 * 4,
            max_readback_bytes: 256 * 64,
        },
    }
}

fn device() -> (wgpu::Device, wgpu::Queue) {
    device_with_limits(wgpu::Limits::default())
}

fn device_with_limits(limits: wgpu::Limits) -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(
        wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        }
        .with_env(),
    );
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("GPU tests require an adapter; configure a supported wgpu backend/device");
    eprintln!("shadow test adapter: {:?}", adapter.get_info());
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
    .expect("GPU test device is required")
}

fn depths(device: &wgpu::Device, readback: &DepthReadback) -> Vec<f32> {
    let (send, receive) = std::sync::mpsc::channel();
    readback
        .buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = readback.buffer.slice(..).get_mapped_range().unwrap();
    let mut values = Vec::new();
    for row in mapped.chunks_exact(readback.bytes_per_row as usize) {
        for texel in row[..readback.resolution as usize * 4].as_chunks::<4>().0 {
            values.push(f32::from_le_bytes(*texel));
        }
    }
    drop(mapped);
    readback.buffer.unmap();
    values
}

fn is_far(depth: f32) -> bool {
    depth.to_bits() == 1.0_f32.to_bits()
}

const TRIANGLE: &str = r"
@vertex fn vs(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let positions = array(vec2(-0.8, -0.8), vec2(0.8, -0.8), vec2(0.0, 0.8));
    return vec4(positions[vertex], 0.25, 1.0);
}
";

fn triangle_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("procedural triangle"),
        source: wgpu::ShaderSource::Wgsl(TRIANGLE.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    create_depth_pipeline(
        device,
        DepthPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            fragment: None,
            compare: wgpu::CompareFunction::Less,
            bias: wgpu::DepthBiasState::default(),
        },
    )
}

fn clear_layers(map: &DepthMap, encoder: &mut wgpu::CommandEncoder) {
    for layer in 0..map.layers() {
        drop(map.begin_layer_pass(encoder, layer, None).unwrap());
    }
}

fn copy_layers(
    map: &DepthMap,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
) -> Vec<DepthReadback> {
    (0..map.layers())
        .map(|layer| map.copy_layer_to_readback(device, encoder, layer).unwrap())
        .collect()
}

fn assert_layer_coverage(device: &wgpu::Device, copies: &[DepthReadback], drawn_layer: u32) {
    for (layer, copy) in copies.iter().enumerate() {
        let values = depths(device, copy);
        assert_eq!(values.len(), 17 * 17);
        if layer <= drawn_layer as usize {
            assert!(values.iter().any(|&depth| (depth - 0.25).abs() < 0.00001));
            assert!(values.iter().any(|&depth| is_far(depth)));
            assert!(
                values
                    .iter()
                    .all(|&depth| is_far(depth) || (depth - 0.25).abs() < 0.00001)
            );
        } else {
            assert!(
                values.iter().all(|&depth| is_far(depth)),
                "draw leaked into layer {layer}"
            );
        }
    }
}

#[test]
fn single_and_four_layer_maps_clear_draw_and_read_independently() {
    let (device, queue) = device();
    let pipeline = triangle_pipeline(&device);
    for (layers, dimension) in [(1, ReceiverDimension::D2), (4, ReceiverDimension::D2Array)] {
        let map = DepthMap::new(&device, descriptor(layers, dimension)).unwrap();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        clear_layers(&map, &mut encoder);
        let clears = copy_layers(&map, &device, &mut encoder);
        queue.submit([encoder.finish()]);
        for readback in &clears {
            assert!(depths(&device, readback).into_iter().all(is_far));
        }
        for drawn_layer in 0..layers {
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = map
                    .begin_layer_pass(&mut encoder, drawn_layer, None)
                    .unwrap();
                pass.set_pipeline(&pipeline);
                pass.draw(0..3, 0..1);
            }
            let copies = copy_layers(&map, &device, &mut encoder);
            queue.submit([encoder.finish()]);
            assert_layer_coverage(&device, &copies, drawn_layer);
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        clear_layers(&map, &mut encoder);
        let resets = copy_layers(&map, &device, &mut encoder);
        queue.submit([encoder.finish()]);
        for reset in &resets {
            assert!(
                depths(&device, reset).into_iter().all(is_far),
                "clear-only pass retained stale depths"
            );
        }
    }
}

#[test]
fn gpu_requests_reject_invalid_allocation_layer_and_disabled_readback() {
    let (device, _queue) = device();
    let mut desc = descriptor(1, ReceiverDimension::D2Array);
    desc.resolution = 0;
    assert!(matches!(
        DepthMap::new(&device, desc),
        Err(DepthMapError::Resolution { .. })
    ));
    desc.resolution = 17;
    desc.readback = false;
    let map = DepthMap::new(&device, desc).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(matches!(
        map.layer_view(1),
        Err(DepthMapError::LayerIndex { .. })
    ));
    assert!(matches!(
        map.copy_layer_to_readback(&device, &mut encoder, 1),
        Err(DepthMapError::LayerIndex { .. })
    ));
    assert!(matches!(
        map.begin_layer_pass(&mut encoder, 1, None),
        Err(DepthMapError::LayerIndex { .. })
    ));
    assert!(matches!(
        map.copy_layer_to_readback(&device, &mut encoder, 0),
        Err(DepthMapError::ReadbackDisabled)
    ));
}

#[test]
fn copies_enforce_retained_budget_and_accept_exact_padded_boundary() {
    let (device, queue) = device();
    for resolution in [17, 64] {
        let copy_bytes = 256 * u64::from(resolution);
        for budget in [0, copy_bytes - 1, copy_bytes] {
            let mut desc = descriptor(1, ReceiverDimension::D2);
            desc.resolution = resolution;
            desc.budget.max_readback_bytes = budget;
            assert_eq!(desc.validate(&device.limits()), Ok(()));
            let map = DepthMap::new(&device, desc).unwrap();
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            clear_layers(&map, &mut encoder);
            let copy = map.copy_layer_to_readback(&device, &mut encoder, 0);
            if budget < copy_bytes {
                assert_eq!(
                    desc.validate_readback(&device.limits()),
                    Err(DepthMapError::ReadbackBudget {
                        required: copy_bytes,
                        limit: budget,
                    })
                );
                assert!(
                    matches!(copy, Err(DepthMapError::ReadbackBudget { required, limit })
                    if required == copy_bytes && limit == budget)
                );
                queue.submit([encoder.finish()]);
            } else {
                assert_eq!(desc.validate_readback(&device.limits()), Ok(()));
                let copy = copy.unwrap();
                assert_eq!(copy.bytes_per_row, 256);
                assert_eq!(copy.buffer.size(), copy_bytes);
                queue.submit([encoder.finish()]);
                let values = depths(&device, &copy);
                assert_eq!(values.len(), resolution as usize * resolution as usize);
                assert!(values.into_iter().all(is_far));
            }
        }
    }
}

#[test]
fn readback_limits_do_not_prevent_texture_allocation_or_rendering() {
    let copy_bytes = 256 * 17;
    let (device, queue) = device_with_limits(wgpu::Limits {
        max_buffer_size: copy_bytes - 1,
        ..wgpu::Limits::default()
    });
    assert_eq!(device.limits().max_buffer_size, copy_bytes - 1);
    let map = DepthMap::new(&device, descriptor(1, ReceiverDimension::D2)).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    clear_layers(&map, &mut encoder);
    assert!(matches!(
        map.copy_layer_to_readback(&device, &mut encoder, 0),
        Err(DepthMapError::ReadbackBudget { required, limit })
            if required == copy_bytes && limit == copy_bytes - 1
    ));
    queue.submit([encoder.finish()]);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
}
