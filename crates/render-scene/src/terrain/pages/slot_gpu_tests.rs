use super::{
    composition::*, mip_gpu::PageWriter, mips::build_mips, residency::*, selection::PageKey,
};
use super::{
    readback_tests::{assert_reads, encode_reads},
    residency_tests::{context, settings},
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::TerrainPageSettings;

#[test]
fn terrain_pages_gpu_slot_layers_do_not_alias_and_reuse_rejects_old_references() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.upload_bytes_per_frame *= 2;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache =
        ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let child = PageKey::new(1, [0, 0]).unwrap();
    let mut frame = authority.begin(device).unwrap();
    let mut reads = Vec::new();
    for (slot, key) in [PageKey::ROOT, child].into_iter().enumerate() {
        let base = constant(settings.extent(), u8::try_from(slot * 180 + 10).unwrap());
        let pending = cache.reserve(&frame, key).unwrap();
        assert_eq!(pending.slot(), u32::try_from(slot).unwrap());
        writer
            .encode(device, &mut frame, &cache, pending, &base)
            .unwrap();
        let expected = build_mips(base).unwrap();
        reads.push((
            encode_reads(
                device,
                &mut frame,
                &cache.storage().texture,
                &expected,
                u32::try_from(slot).unwrap(),
            ),
            expected,
        ));
    }
    frame.finish().submit().unwrap();
    let old = cache.lookup(child, 0).unwrap();
    assert_eq!(old.slot(), 1);
    assert_eq!(cache.lookup(PageKey::new(2, [0, 0]).unwrap(), 0), Some(old));
    for (read, expected) in &reads {
        assert_reads(device, read, expected);
    }
    cache.invalidate([0.99, 0.99], [1.0, 1.0]);
    assert!(cache.lookup(PageKey::ROOT, 0).is_none());
    assert!(cache.is_current(old));
    cache.evict(child);
    assert!(!cache.is_current(old));
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, child).unwrap();
    assert_eq!(pending.slot(), 0);
    let base = constant(settings.extent(), 77);
    writer
        .encode(device, &mut frame, &cache, pending, &base)
        .unwrap();
    let expected = build_mips(base).unwrap();
    let reads = encode_reads(device, &mut frame, &cache.storage().texture, &expected, 0);
    frame.finish().submit().unwrap();
    assert!(!cache.is_current(old));
    assert_reads(device, &reads, &expected);
}

fn constant(extent: [u32; 2], value: u8) -> MaterialImage {
    MaterialImage::new(
        extent,
        vec![
            MaterialTexel {
                color: [value; 4],
                normal_roughness: [128, 128, 255, value]
            };
            usize::try_from(extent[0] * extent[1]).unwrap()
        ],
    )
    .unwrap()
}
