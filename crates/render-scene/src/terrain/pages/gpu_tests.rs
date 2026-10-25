use super::{composition::*, gpu::*};
use worldrunner_render_post::{StageId, contracts::*};

#[test]
fn terrain_pages_gpu_composition_matches_exact_cpu_products() {
    let context = pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))
    .unwrap();
    let sample = |color, roughness, weight| LayerSample {
        color,
        normal: [0, 0, i16::MAX],
        roughness,
        weight,
    };
    let patterns = [
        vec![
            sample([0, 64, 128, 255], 0, 1),
            sample([200, 128, 0, 255], 200, 3),
        ],
        vec![sample([16, 32, 64, 255], 80, u16::MAX)],
        vec![
            sample([255, 0, 255, 255], 255, 1),
            sample([0, 255, 0, 255], 0, 1),
        ],
        vec![
            LayerSample {
                normal: [i16::MAX, 0, 0],
                ..sample([64; 4], 80, 1)
            },
            sample([64; 4], 80, 1),
        ],
        vec![sample([255; 4], 255, u16::MAX); 256],
    ];
    let samples: Vec<_> = patterns.into_iter().cycle().take(35).collect();
    let batch = CompositionBatch::new(&samples).unwrap();
    let expected: Vec<u8> = samples
        .iter()
        .flat_map(|layers| {
            let texel = compose_texel(layers).unwrap();
            [texel.color, texel.normal_roughness].concat()
        })
        .collect();
    assert_eq!(execute(&context, &batch), expected);
}

fn execute(context: &crate::device::GpuContext, batch: &CompositionBatch) -> Vec<u8> {
    let device = context.device();
    let mut registry = ResourceRegistry::default();
    let ranges = storage(device, batch.ranges().len());
    let samples = storage(device, batch.samples().len());
    let output = storage(device, usize::try_from(batch.output_bytes()).unwrap());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("material composition readback"),
        size: batch.output_bytes(),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let views = CompositionViews {
        ranges: import(&mut registry, &ranges),
        samples: import(&mut registry, &samples),
        output: import(&mut registry, &output),
    };
    let destination = import(&mut registry, &readback);
    let operations = vec![
        upload("page ranges", views.ranges.clone(), batch.ranges()),
        upload("page samples", views.samples.clone(), batch.samples()),
        batch.operation(device, &registry, views.clone()).unwrap(),
        TypedOperation::new(
            id("page readback"),
            TypedWork::ReadbackBuffer {
                source: views.output,
                destination,
            },
        ),
    ];
    let compiled = registry.compile_operations(device, &operations).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    context.queue().submit([encoder.finish()]);
    read(device, &readback)
}

fn storage(device: &wgpu::Device, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("material composition buffer"),
        size: u64::try_from(size).unwrap(),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn import(registry: &mut ResourceRegistry, buffer: &wgpu::Buffer) -> BufferView {
    let key = registry
        .import_buffer(buffer, ResourceLifetime::FrameLocal, None)
        .unwrap();
    registry.buffer_view(key, 0..buffer.size(), 1).unwrap()
}

fn id(name: &'static str) -> RasterId {
    RasterId::new(StageId::new(name, 0), 0)
}
fn upload(name: &'static str, destination: BufferView, bytes: &[u8]) -> TypedOperation {
    TypedOperation::new(
        id(name),
        TypedWork::Upload {
            destination,
            bytes: bytes.to_vec(),
        },
    )
}

pub(super) fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    let (send, receive) = std::sync::mpsc::channel();
    buffer
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
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let bytes = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    bytes
}
