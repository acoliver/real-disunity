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

/// The reference filter is a different shader program, and a driver may contract or reorder
/// its normal arithmetic differently: RADV differed by up to 1.2e-7, even where both read level
/// 0 only. One step of an 8-bit normal channel is about 7.8e-3, so a wrong level still fails.
const NORMAL_TOLERANCE: f32 = 1e-6;

fn assert_normals_close(current: &[u8], reference: &[u8]) {
    assert_eq!(current.len(), reference.len());
    for (index, (current, reference)) in current
        .chunks_exact(4)
        .zip(reference.chunks_exact(4))
        .enumerate()
    {
        let current = component(current, 0);
        let reference = component(reference, 0);
        assert!(
            (current - reference).abs() <= NORMAL_TOLERANCE,
            "normal component {index}: {current} versus {reference}"
        );
    }
}

#[test]
fn terrain_pages_gpu_single_level_filter_matches_two_level_mix() {
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
        if normals {
            assert_normals_close(&current, &reference);
        } else {
            assert_eq!(current, reference, "colour bytes");
        }
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
    // Points 9 to 17 have fractional footprints; each holds four f32 components.
    let fractional = (3 * 3 * 4)..(3 * 6 * 4);
    let largest = fractional
        .map(|i| component(&blended, i) - component(&level_zero, i))
        .fold(0.0_f32, |largest, difference| largest.max(difference.abs()));
    assert!(
        largest > 1000.0 * NORMAL_TOLERANCE,
        "fractional footprints sample a second level: largest normal change {largest}"
    );
}

fn component(bytes: &[u8], index: usize) -> f32 {
    f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
}
