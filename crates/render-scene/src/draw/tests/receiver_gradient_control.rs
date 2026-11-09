//! Streamed terrain places different receiver planes in adjacent draw calls.
use super::*;

fn source() -> RenderScene {
    let source = sloped(false);
    let ground = source.terrain().unwrap();
    let side = 513_u32;
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: side,
        rows: side,
        origin_east_millimeters: OFFSET[0] - 307_200,
        origin_north_millimeters: OFFSET[1] - 307_200,
        sample_spacing_millimeters: 1_200,
        heights_millimeters: (0..side * side)
            .map(|i| {
                let x = i % side;
                let y = i / side;
                100_000
                    + (i32::try_from(y).unwrap() - 256) * 160
                    + i32::try_from((x * 17 + y * 23 + x * y * 31) % 4_500).unwrap()
            })
            .collect(),
        surface_indices: vec![0; (side * side) as usize],
        surfaces: ground.surfaces().to_vec(),
        lod_policy: ground.lod_policy(),
    })
    .unwrap()
    .with_streaming_policy(Some(
        worldrunner_render_plan::TerrainStreamingPolicy::new(8, 4096, 4096, 500).unwrap(),
    ));
    let mut parts = effect_parts();
    parts.ssao = Some(crate::ssr::neutral_ao());
    parts.sun = *source.effects().unwrap().sun();
    parts.shadows = source.effects().unwrap().shadows().clone();
    source
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
        .with_objects_and_effects(Vec::new(), Some(SceneEffects::new(parts).unwrap()))
        .unwrap()
}

pub(super) fn control(context: &GpuContext) -> usize {
    let source = source();
    let pose = CameraPose {
        eye_millimeters: [OFFSET[0], OFFSET[1] - 40_000, 120_000],
        target_millimeters: [OFFSET[0], OFFSET[1] + 50_000, 100_000],
    };
    let stored = frame_at_pose(
        context,
        &source,
        Some(ReceiverReference::StoredProbeReuse),
        0.0,
        pose,
    );
    for reference in [
        ReceiverReference::StoredProbeShared,
        ReceiverReference::StoredProbeIndependent,
    ] {
        let selected = frame_at_pose(context, &source, Some(reference), 0.0, pose);
        let differences: Vec<_> = selected
            .iter()
            .zip(&stored)
            .map(|(a, b)| {
                assert_eq!(a.len(), b.len());
                a.iter().zip(b).filter(|(x, y)| x != y).count()
            })
            .collect();
        println!("STREAMED_STORED_CONTROL {reference:?} attachment_bytes={differences:?}");
        assert!(
            selected == stored,
            "streamed stored receiver attachments must match: {reference:?}"
        );
    }
    assert_plane_consistency(context, &source, pose, &stored);
    let reuse = frame_at_pose(
        context,
        &source,
        Some(ReceiverReference::HardwareProbeReuse),
        0.0,
        pose,
    );
    [
        ReceiverReference::HardwareProbeShared,
        ReceiverReference::HardwareProbeIndependent,
    ]
    .into_iter()
    .map(|reference| {
        let local = frame_at_pose(context, &source, Some(reference), 0.0, pose);
        assert_eq!(local.len(), reuse.len());
        let differences: Vec<_> = local
            .iter()
            .zip(&reuse)
            .map(|(a, b)| {
                assert_eq!(a.len(), b.len());
                a.iter().zip(b).filter(|(x, y)| x != y).count()
            })
            .collect();
        println!("STREAMED_PROBE_CONTROL {reference:?} attachment_bytes={differences:?}");
        differences.iter().sum::<usize>()
    })
    .sum()
}
#[test]
fn gpu_streamed_receiver_payload_survives_prior_terrain_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = sloped(false);
    let mut prior = ReceiverFrame::new(&context, &source, None, 0.0);
    for reference in [
        ReceiverReference::StoredProbeShared,
        ReceiverReference::StoredProbeReuse,
        ReceiverReference::StoredProbeIndependent,
    ] {
        prior.capture(&context, &source, Some(reference), receiver_pose());
    }
    drop(prior);
    assert!(control(&context) > 0);
}

fn assert_plane_consistency(
    context: &GpuContext,
    source: &RenderScene,
    pose: CameraPose,
    stored: &[Vec<u8>],
) {
    for reference in [
        ReceiverReference::ProbeShared,
        ReceiverReference::ProbeReuse,
        ReceiverReference::ProbeIndependent,
    ] {
        let local = frame_at_pose(context, source, Some(reference), 0.0, pose);
        assert_eq!(local.len(), stored.len());
        for (attachment, (a, b)) in local.iter().zip(stored).enumerate() {
            assert_eq!(a.len(), b.len());
            let differences = a.iter().zip(b).filter(|(x, y)| x != y).count();
            println!(
                "TERRAIN_PRODUCER_LOCAL {reference:?} attachment={attachment} differing_bytes={differences}"
            );
            assert_eq!(
                differences, 0,
                "selected-plane terrain receivers must agree at seams: {reference:?} attachment={attachment}"
            );
        }
    }
}

#[test]
fn gpu_terrain_producer_and_local_receivers_agree_at_triangle_and_chunk_seams() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = source();
    let pose = CameraPose {
        eye_millimeters: [OFFSET[0], OFFSET[1] - 40_000, 120_000],
        target_millimeters: [OFFSET[0], OFFSET[1] + 50_000, 100_000],
    };
    let stored = frame_at_pose(
        &context,
        &source,
        Some(ReceiverReference::StoredProbeReuse),
        0.0,
        pose,
    );
    assert_plane_consistency(&context, &source, pose, &stored);
}
