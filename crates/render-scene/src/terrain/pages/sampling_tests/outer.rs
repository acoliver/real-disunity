use super::*;

#[test]
fn terrain_pages_gpu_outer_edge_keeps_exact_endpoint_at_maximum_depth() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.maximum_depth = 16;
    parts.interior = [4, 4];
    parts.border = 2;
    parts.upload_bytes_per_frame = 1000;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let cache =
        ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let leaf = PageKey::new(16, [65_535, 0]).unwrap();
    for key in [PageKey::ROOT, leaf] {
        let texels = (0..64_u32)
            .map(|index| MaterialTexel {
                color: [
                    if key == leaf {
                        u8::try_from(((index % 8) * 48).saturating_sub(40).min(255)).unwrap()
                    } else {
                        64
                    },
                    128,
                    192,
                    255,
                ],
                normal_roughness: [128, 128, 255, 180],
            })
            .collect();
        let base = MaterialImage::new(settings.extent(), texels).unwrap();
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, key).unwrap();
        writer
            .encode(device, &mut frame, &cache, pending, &base)
            .unwrap();
        let submission = frame.finish().submit().unwrap();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();
    }
    let bytes = harness::sample(
        device,
        &authority,
        &cache,
        &[leaf],
        &[[1.0, 0.25 / 65_536.0, 0.0, 0.0]],
    );
    assert_eq!(bytes, [224, 128, 192, 255]);
}
