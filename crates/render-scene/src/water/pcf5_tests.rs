use super::*;
use worldrunner_render_plan::{SceneWaterShadowFilter, SceneWaterShadowResponse};

fn axis(phase: f64) -> ([f64; 3], [f64; 3]) {
    let weights = [4.0 - 3.0 * phase, 7.0, 1.0 + 3.0 * phase];
    let offsets = [
        (3.0 - 2.0 * phase) / weights[0] - 2.0,
        (3.0 + phase) / 7.0,
        phase / weights[2] + 2.0,
    ];
    (weights, offsets)
}

#[test]
fn weighted_pairs_expand_to_phase_shifted_five_texel_kernel() {
    for step in 0..=1000 {
        let phase = f64::from(step) / 1000.0;
        let (weights, offsets) = axis(phase);
        let mut expanded = [0.0; 6];
        for (i, (weight, offset)) in weights.into_iter().zip(offsets).enumerate() {
            let fraction = offset - [-2.0, 0.0, 2.0][i];
            expanded[2 * i] = weight * (1.0 - fraction);
            expanded[2 * i + 1] = weight * fraction;
        }
        let expected = [
            1.0 - phase,
            3.0 - 2.0 * phase,
            4.0 - phase,
            3.0 + phase,
            1.0 + 2.0 * phase,
            phase,
        ];
        assert!((weights.iter().sum::<f64>() - 12.0).abs() < 1e-12);
        for (actual, expected) in expanded.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 2e-15);
        }
    }
}

#[test]
fn pcf5_is_opt_in_and_per_body_without_changing_default_source() {
    let mut body = settings();
    let source = water_source(true);
    let baseline = terms::uniforms(&body, 0).map(f32::to_bits);
    assert_eq!(shadow_response::specialize(&source, 0, true), source);
    assert_eq!(
        shadow_response::specialize(&source, shadow_response::SPECIALIZATION, true),
        source
    );
    body.material.shadow_response = Some(SceneWaterShadowResponse::default());
    assert_eq!(terms::uniforms(&body, 0).map(f32::to_bits), baseline);
    assert_eq!(terms::mask(&[body.clone()]), 0);
    body.material.shadow_response.as_mut().unwrap().filter = SceneWaterShadowFilter::WeightedPcf5;
    let key = terms::mask(&[body.clone()]);
    let filtered = shadow_response::specialize(&source, key, true);
    assert!(filtered.contains("rd_shadow_array_weighted_pcf5"));
    assert!(
        filtered.contains(
            "water_filtered_raw_sunlight_visibility(position, water_shadow_normal(normal))"
        )
    );
    assert!(filtered.contains("let ambient_visibility = raw_sunlight_visibility"));
    assert_eq!(terms::uniforms(&body, 0)[31].to_bits(), 4.0f32.to_bits());
    assert_eq!(terms::uniforms(&settings(), 0).map(f32::to_bits), baseline);
    let unshadowed = water_source(false);
    assert_eq!(
        shadow_response::specialize(&unshadowed, key, false),
        unshadowed
    );
    body.material.shadow_response = Some(SceneWaterShadowResponse {
        diffuse_floor_micro: 600_000,
        ..Default::default()
    });
    assert_eq!(
        terms::mask(&[body.clone()]),
        shadow_response::SPECIALIZATION
    );
    assert_eq!(terms::uniforms(&body, 0)[31].to_bits(), 0.0f32.to_bits());
}
