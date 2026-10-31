use super::*;

const TOLERANCE: f64 = 0.012;

#[test]
fn gpu_water_pcf5_receiver_matches_oracle_and_rejects_bilinear_in_both_paths() {
    let context = context();
    let source = scene(&[], 600_000, None);
    let fixture = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            resolution: 256,
            ..settings()
        },
    )
    .unwrap();
    let pipeline = super::water_pcf5::binary_pipeline(context.device(), false);
    super::water_pcf5::binary_probe(&context, &fixture, &pipeline, 128.5);
    let shader = visibility_source();
    for scene_path in [false, true] {
        for q in [[128.5, 128.5], [128.625, 128.625], [128.75, 128.5]] {
            constant_projection(&context, &fixture, q);
            let expected = 0.2 + 0.8 * super::water_pcf5::oracle(q[0], q[1]);
            let bilinear = 0.2 + 0.8 * bilinear(q);
            assert!((expected - bilinear).abs() > 2.0 * TOLERANCE);
            let bytes = super::water_identity::capture_shadow(
                &context,
                Some(SceneWaterShadowResponse {
                    filter: SceneWaterShadowFilter::WeightedPcf5,
                    ..Default::default()
                }),
                scene_path,
                false,
                Some(&shader),
                Some(&fixture.map.group),
            );
            assert_interior(&bytes, expected, bilinear);
        }
    }
}

fn visibility_source() -> String {
    let source = crate::water::water_source(true);
    // Observe the production water receiver after strength/response handling.
    // The other channels identify water without accepting opaque or MSAA edges.
    source.replace(
        "vec4<f32>(color, 1.0)",
        "vec4<f32>(visibility, 0.125, 0.25, 1.0)",
    )
}

fn constant_projection(context: &GpuContext, fixture: &ShadowedScene, q: [f64; 2]) {
    super::selection::upload_probe(context, fixture, true, 1, 0.0);
    let translation = [q[0] / 128.0 - 1.0, 1.0 - q[1] / 128.0, 0.5, 1.0];
    context.queue().write_buffer(
        &fixture.map.uniform,
        48,
        &translation
            .map(|value| format!("{value}").parse::<f32>().unwrap())
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    // One coverage-selected map with no bias or plane gradient, 80% strength.
    for (offset, value) in [(268, 0.2f32), (280, 0.0f32)] {
        context
            .queue()
            .write_buffer(&fixture.map.uniform, offset, &value.to_le_bytes());
    }
}

fn bilinear(q: [f64; 2]) -> f64 {
    let base = q.map(|value| (value - 0.5).floor());
    let phase = [q[0] - 0.5 - base[0], q[1] - 0.5 - base[1]];
    let mut value = 0.0;
    for y in 0..2 {
        for x in 0..2 {
            let tx = base[0] + f64::from(x);
            let ty = base[1] + f64::from(y);
            let lit = f64::from(u8::from((tx + 3.0 * ty + tx * ty) % 7.0 >= 3.0));
            let wx = if x == 0 { 1.0 - phase[0] } else { phase[0] };
            let wy = if y == 0 { 1.0 - phase[1] } else { phase[1] };
            value += lit * wx * wy;
        }
    }
    value
}

fn assert_interior(bytes: &[u8], expected: f64, bilinear: f64) {
    let pixels = bytes.chunks_exact(8).collect::<Vec<_>>();
    let water = |index: usize| pixels[index][2..6] == [0x00, 0x30, 0x00, 0x34];
    let mut count = 0;
    for y in 32..224 {
        for x in 32..224 {
            let index = y * 256 + x;
            if [index, index - 1, index + 1, index - 256, index + 256]
                .into_iter()
                .all(water)
            {
                let actual =
                    half::f16::from_le_bytes([pixels[index][0], pixels[index][1]]).to_f64();
                assert!(
                    (actual - expected).abs() < TOLERANCE,
                    "water ({x},{y}): {actual} vs PCF5 {expected}, bilinear {bilinear}"
                );
                assert!((actual - bilinear).abs() > TOLERANCE);
                count += 1;
            }
        }
    }
    assert!(count > 64, "no water interior was checked: {count}");
}
