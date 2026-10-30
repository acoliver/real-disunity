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
    let actual = execute(&context, &batch);
    assert_eq!(actual.len(), expected.len());
    for (index, (layers, (actual, expected))) in samples
        .iter()
        .zip(actual.chunks_exact(8).zip(expected.chunks_exact(8)))
        .enumerate()
    {
        // Colour and roughness are weighted means in integer arithmetic, which is exact on any
        // device, so those five bytes equal the CPU products.
        assert_eq!(actual[..4], expected[..4], "texel {index} colour");
        assert_eq!(actual[7], expected[7], "texel {index} roughness");
        let ideal = ideal_normal_codes(layers);
        for (axis, (&code, ideal)) in actual[4..7].iter().zip(ideal).enumerate() {
            // The normal is float arithmetic (up to 256 normalised terms summed, then normalised)
            // whose last bits differ between devices. The device code is within the 8-bit rounding
            // step (0.5) plus the f32 rounding of about 320 operations on a value of at most one,
            // times 255 and a factor of five for GPU divide and square root accuracy.
            let bound =
                0.5 + 255.0 * 5.0 * worldrunner_render_gpu::tolerance_test_support::ARITHMETIC;
            assert!(
                (f64::from(code) - ideal).abs() <= bound,
                "texel {index} normal axis {axis}: got {code}, ideal {ideal}, bound {bound}"
            );
        }
    }
}

/// Unquantised encoded normal codes of the weighted normalised normals, in f64.
fn ideal_normal_codes(layers: &[LayerSample]) -> [f64; 3] {
    let mut sum = [0.0_f64; 3];
    for layer in layers.iter().filter(|layer| layer.weight > 0) {
        let n = layer.normal.map(f64::from);
        let length = n.iter().map(|v| v * v).sum::<f64>().sqrt();
        for (total, v) in sum.iter_mut().zip(n) {
            *total += v / length * f64::from(layer.weight);
        }
    }
    let length = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
    sum.map(|v| ((v / length) * 0.5 + 0.5) * 255.0)
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
