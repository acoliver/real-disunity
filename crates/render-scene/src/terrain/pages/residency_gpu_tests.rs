use super::readback_tests::{assert_reads, encode_reads};
use super::residency_tests::{context, settings};
use super::{composition::*, gpu::*, mip_gpu::*, residency::*, selection::PageKey};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_post::{StageId, contracts::*};

#[test]
fn terrain_pages_composition_to_resident_and_cancelled_work_never_publishes() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let settings = settings();
    let cache =
        ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let samples = samples();
    let batch = CompositionBatch::new(&samples).unwrap();
    let base = MaterialImage::new(
        [5, 7],
        samples
            .iter()
            .map(|layers| compose_texel(layers).unwrap())
            .collect(),
    )
    .unwrap();
    let expected = super::mips::build_mips(base).unwrap();
    for cancel in [true, false] {
        let mut registry = ResourceRegistry::default();
        let (source, producers) = producers(device, &mut registry, &batch);
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
        writer
            .encode_prepared(
                device,
                &mut frame,
                &cache,
                PageInput {
                    registry: &mut registry,
                    pending,
                    source,
                    producers,
                },
            )
            .unwrap();
        assert!(cache.lookup(PageKey::ROOT, 0).is_none());
        if cancel {
            frame.finish().cancel();
            assert!(cache.lookup(PageKey::ROOT, 0).is_none());
        } else {
            let reads = encode_reads(device, &mut frame, &cache.storage().texture, &expected, 0);
            frame.finish().submit().unwrap();
            assert!(cache.lookup(PageKey::ROOT, 0).is_some());
            assert_reads(device, &reads, &expected);
        }
    }
}
fn samples() -> Vec<Vec<LayerSample>> {
    (0..35)
        .map(|i| {
            vec![
                LayerSample {
                    color: [i, 128, 64, 255],
                    normal: [0, 0, i16::MAX],
                    roughness: i * 7,
                    weight: 1,
                },
                LayerSample {
                    color: [200, 20, 10, 255],
                    normal: [i16::MAX, 0, i16::MAX],
                    roughness: 180,
                    weight: 3,
                },
            ]
        })
        .collect()
}
pub(super) fn producers(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    batch: &CompositionBatch,
) -> (BufferView, Vec<TypedOperation>) {
    let ranges = buffer(device, registry, batch.ranges().len() as u64);
    let layers = buffer(device, registry, batch.samples().len() as u64);
    let output = buffer(device, registry, batch.output_bytes());
    let views = CompositionViews {
        ranges: ranges.clone(),
        samples: layers.clone(),
        output: output.clone(),
    };
    let producers = vec![
        upload("page ranges", ranges, batch.ranges()),
        upload("page layers", layers, batch.samples()),
        batch.operation(device, registry, views).unwrap(),
    ];
    (output, producers)
}
fn buffer(device: &wgpu::Device, registry: &mut ResourceRegistry, size: u64) -> BufferView {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("page composition test"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let key = registry
        .import_buffer(&buffer, ResourceLifetime::FrameLocal, None)
        .unwrap();
    registry.buffer_view(key, 0..size, 1).unwrap()
}
fn upload(name: &'static str, destination: BufferView, bytes: &[u8]) -> TypedOperation {
    TypedOperation::new(
        RasterId::new(StageId::new(name, 0), 0),
        TypedWork::Upload {
            destination,
            bytes: bytes.to_vec(),
        },
    )
}
