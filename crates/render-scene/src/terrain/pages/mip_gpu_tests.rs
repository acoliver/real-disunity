use super::readback_tests::{assert_reads, encode_reads};
use super::residency_tests::{context, settings};
use super::{
    composition::*, mip_gpu::PageWriter, mips::build_mips, residency::*, selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::TerrainPageSettings;

#[test]
fn terrain_pages_gpu_every_mip_matches_cpu_including_odd_borders() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    for interior in [[3, 5], [5, 7], [1, 1], [2, 4], [7, 1]] {
        let mut parts = settings().parts();
        parts.interior = interior;
        parts.upload_bytes_per_frame = 100_000;
        parts.upload_bytes_per_frame = TerrainPageSettings::new(parts).unwrap().page_bytes();
        let settings = TerrainPageSettings::new(parts).unwrap();
        let writer = PageWriter::new(device, settings);
        let cache =
            ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
        let base = pattern(settings.extent());
        let expected = build_mips(base.clone()).unwrap();
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
        writer
            .encode(device, &mut frame, &cache, pending, &base)
            .unwrap();
        let reads = encode_reads(device, &mut frame, &cache.storage().texture, &expected, 0);
        assert!(cache.lookup(PageKey::ROOT, 0).is_none());
        frame.finish().submit().unwrap();
        assert!(cache.lookup(PageKey::ROOT, 0).is_some());
        assert_reads(device, &reads, &expected);
    }
}

fn pattern([w, h]: [u32; 2]) -> MaterialImage {
    let texels = (0..w * h)
        .map(|i| MaterialTexel {
            color: [byte(i * 37), byte(i * 17), byte(i * 7), 255],
            normal_roughness: if i % 2 == 0 {
                [218, 128, 218, byte(i * 13)]
            } else {
                [128, 128, 255, byte(i * 13)]
            },
        })
        .collect();
    MaterialImage::new([w, h], texels).unwrap()
}
fn byte(value: u32) -> u8 {
    u8::try_from(value % 256).unwrap()
}
