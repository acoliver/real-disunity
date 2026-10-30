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
        assert_ramp_across_gutter(&bytes, derivative);
    }
}

/// The red ramp is `32 + 192 u` (116, 128 and 140 at the three points), the other channels are
/// constant. A bilinear filter of texels that sit on an affine ramp returns the ramp at the sample
/// point (the weighted centre of the texels), so the filtered value differs from the ideal ramp
/// only by: the 8-bit store of each texel and of each mip generation (0.5 code each, at most two
/// generations here: 1.0), the final 8-bit store (0.5), and the filter weight quantisation. Vulkan
/// guarantees only 4 bits of sub-texel precision, so a weight errs by up to 2^-4 times the texel
/// pitch, at most 48 codes for a root texel (0.25 of the ramp's 192 codes): 3.0. The total is
/// 4.5 codes. A missing gutter would clamp the ramp flat or jump by a whole pitch and fail the
/// strict monotonic order or the bound. Constant channels are exact copies of a constant texel.
fn assert_ramp_across_gutter(bytes: &[u8], derivative: f32) {
    const BOUND: f64 = 0.5 + 1.0 + 3.0;
    let ideal = [116.0, 128.0, 140.0];
    let mut previous = None;
    for (point, want) in ideal.into_iter().enumerate() {
        let pixel = &bytes[point * 4..point * 4 + 4];
        assert!(
            (f64::from(pixel[0]) - want).abs() <= BOUND,
            "LOD derivative {derivative}, point {point}: got {}, ideal {want}, bound {BOUND}",
            pixel[0]
        );
        assert_eq!(
            &pixel[1..],
            [64, 192, 255],
            "LOD derivative {derivative}, point {point}: constant channels"
        );
        if let Some(before) = previous {
            assert!(
                pixel[0] > before,
                "LOD derivative {derivative}, point {point}: ramp not strictly increasing across the gutter"
            );
        }
        previous = Some(pixel[0]);
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
