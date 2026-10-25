use super::{
    composition::{MaterialImage, MaterialTexel},
    mip_gpu::PageWriter,
    residency::ResidentPages,
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::TerrainPageSettings;

#[test]
fn populated_tree_gpu_lookup_reads_one_record_per_depth() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.interior = [1, 1];
    parts.maximum_selected_pages = 64;
    parts.upload_bytes_per_frame = 100_000;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache = ResidentPages::new(
        device,
        &authority,
        settings,
        85,
        AllocationBudget(1_000_000),
    )
    .unwrap();
    let writer = PageWriter::new(device, settings);
    let base = MaterialImage::new(
        [3, 3],
        vec![
            MaterialTexel {
                color: [64, 128, 192, 255],
                normal_roughness: [128, 128, 255, 180],
            };
            9
        ],
    )
    .unwrap();
    let leaves = populate(device, &authority, &cache, &writer, &base);
    let records = super::scene::table::records(&leaves, &cache.residents());
    assert_eq!(records.len(), 85);
    let points: Vec<_> = (0..8_u16)
        .flat_map(|y| {
            (0..8_u16).map(move |x| {
                [
                    (f32::from(x) + 0.5) / 8.0,
                    (f32::from(y) + 0.5) / 8.0,
                    0.0,
                    0.0,
                ]
            })
        })
        .collect();
    let bytes = super::sampling_tests::harness::sample_metric(
        device, &authority, &cache, &leaves, &points, true,
    );
    for count in bytes.chunks_exact(4) {
        assert_eq!(
            u32::from_le_bytes(count.try_into().unwrap()),
            4,
            "lookup work must depend on depth, not 64 leaves or 85 residents"
        );
    }
}

fn populate(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    cache: &ResidentPages,
    writer: &PageWriter,
    base: &MaterialImage,
) -> Vec<PageKey> {
    let mut frame = authority.begin(device).unwrap();
    let mut leaves = Vec::new();
    for depth in 0..=3 {
        for y in 0..1 << depth {
            for x in 0..1 << depth {
                let key = PageKey::new(depth, [x, y]).unwrap();
                let pending = cache.reserve(&frame, key).unwrap();
                writer
                    .encode(device, &mut frame, cache, pending, base)
                    .unwrap();
                if depth == 3 {
                    leaves.push(key);
                }
            }
        }
    }
    frame.finish().submit().unwrap();
    leaves
}
