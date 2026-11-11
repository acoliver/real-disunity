use super::*;

/// A root page whose base texels vary, so its mip levels differ from each other.
fn varied_root(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
) -> (ResidentPages, worldrunner_render_plan::TerrainPageSettings) {
    let mut parts = settings().parts();
    parts.interior = [4, 4];
    parts.border = 2;
    parts.upload_bytes_per_frame = 1000;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let cache =
        ResidentPages::new(device, authority, settings, 2, AllocationBudget(100_000)).unwrap();
    let texels = (0..64_u32)
        .map(|index| {
            let channel = |scale: u32| u8::try_from((index * scale + 17) % 251).unwrap();
            MaterialTexel {
                color: [channel(37), channel(91), channel(13), 255],
                normal_roughness: [channel(53), channel(29), 220, channel(71)],
            }
        })
        .collect();
    let base = MaterialImage::new(settings.extent(), texels).unwrap();
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    PageWriter::new(device, settings)
        .encode(device, &mut frame, &cache, pending, &base)
        .unwrap();
    let submission = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
    (cache, settings)
}

#[test]
fn terrain_pages_gpu_single_level_filter_matches_two_level_mix_bit_for_bit() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let (cache, settings) = varied_root(device, &authority);
    assert!(
        settings.maximum_sampling_mip() >= 1,
        "fixture exercises a second level"
    );
    // Footprints of 0 to 1 texel select level 0 exactly; larger ones blend two levels.
    let points: Vec<[f32; 4]> = [0.0_f32, 0.5, 1.0, 1.1, 1.5, 1.9, 2.0, 2.6, 3.7]
        .into_iter()
        .flat_map(|texels| {
            let world = texels / 4.0;
            [0.13_f32, 0.41, 0.77]
                .into_iter()
                .map(move |x| [x, 1.0 - x * 0.6, world, world])
        })
        .collect();
    for normals in [false, true] {
        let mode = harness::Mode {
            work: false,
            normals,
            level: 1,
            baseline: false,
            extent: [1.0; 2],
        };
        let leaves = [PageKey::ROOT];
        let current = harness::sample_with(device, &authority, &cache, &leaves, &points, mode);
        let reference =
            harness::sample_two_level(device, &authority, &cache, &leaves, &points, mode);
        assert_eq!(current, reference, "normals {normals}");
    }
    // The fractional footprints really blend distinct levels, so a wrong skip would show.
    let mode = harness::Mode {
        work: false,
        normals: true,
        level: 1,
        baseline: false,
        extent: [1.0; 2],
    };
    let blended = harness::sample_with(device, &authority, &cache, &[PageKey::ROOT], &points, mode);
    let level_zero = harness::sample_with(
        device,
        &authority,
        &cache,
        &[PageKey::ROOT],
        &points
            .iter()
            .map(|p| [p[0], p[1], 0.0, 0.0])
            .collect::<Vec<_>>(),
        mode,
    );
    let stride = 16;
    let fractional = 3 * 3..3 * 6;
    assert!(
        fractional
            .clone()
            .any(|i| blended[i * stride..(i + 1) * stride]
                != level_zero[i * stride..(i + 1) * stride]),
        "fractional footprints sample a second level"
    );
}
