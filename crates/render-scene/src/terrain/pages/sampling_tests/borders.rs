use super::*;
use crate::terrain::pages::composition::{LayerSample, compose_page};

#[test]
fn terrain_pages_gpu_filters_neighbor_gutters_and_fractional_mips_without_seams() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.interior = [4, 4];
    parts.border = 2;
    parts.upload_bytes_per_frame = 1000;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let cache =
        ResidentPages::new(device, &authority, settings, 5, AllocationBudget(100_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let leaves =
        [[0, 0], [1, 0], [0, 1], [1, 1]].map(|coordinate| PageKey::new(1, coordinate).unwrap());
    for key in [PageKey::ROOT].into_iter().chain(leaves) {
        let base = compose_page(settings, key, ramp).unwrap();
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
    for derivative in [0.0, 0.125, 0.176_776_69, 0.25] {
        let points = [
            [0.4375, 0.25, derivative, 0.0],
            [0.5, 0.25, derivative, 0.0],
            [0.5625, 0.25, derivative, 0.0],
        ];
        let bytes = harness::sample(device, &authority, &cache, &leaves, &points);
        assert_eq!(
            bytes,
            [116, 64, 192, 255, 128, 64, 192, 255, 140, 64, 192, 255],
            "LOD derivative {derivative}"
        );
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped linear UNORM source"
)]
fn ramp(uv: [f64; 2]) -> Vec<LayerSample> {
    vec![LayerSample {
        color: [
            (32.0 + uv[0] * 192.0).round().clamp(0.0, 255.0) as u8,
            64,
            192,
            255,
        ],
        normal: [0, 0, i16::MAX],
        roughness: 180,
        weight: 1,
    }]
}
