use super::*;
use worldrunner_render_plan::{SceneRoughnessRemap, SceneSpecularVisibility};

#[test]
fn visibility_selection_does_not_change_low_roughness_distribution() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for remap in [
        SceneRoughnessRemap::Perceptual045,
        SceneRoughnessRemap::Perceptual04,
    ] {
        let mut outputs = Vec::new();
        for visibility in [
            SceneSpecularVisibility::SmithCorrelated,
            SceneSpecularVisibility::SchlickSmith,
        ] {
            let mut frame = configured_frame(ExposureOwnership::SceneLinear(1.0));
            frame.specular_convention.roughness = remap;
            frame.specular_convention.visibility = visibility;
            outputs.push(evaluate(
                &context,
                r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let r = scene_roughness(0.01);
    result[0] = vec4(scene_distribution_ggx(1.0,r), scene_distribution_ggx(0.999999,r),
        rd_visibility(0.8,0.7,r,SCENE_SPECULAR_VISIBILITY),r);
}",
                Some(&frame),
            ));
        }
        assert_eq!(outputs[0][..2], outputs[1][..2]);
        assert!((outputs[0][2] - outputs[1][2]).abs() > 0.001);
        let r = f64::from(outputs[0][3]);
        for (i, nh) in [1.0_f32, 0.999_999].into_iter().enumerate() {
            let denominator = f64::from(nh).powi(2) * (r.powi(4) - 1.0) + 1.0;
            let expected = if remap == SceneRoughnessRemap::Perceptual045 {
                r.powi(4) / (std::f64::consts::PI * denominator.powi(2)).max(1e-7)
            } else {
                (r.powi(2) / denominator.max(1e-8)).powi(2) / std::f64::consts::PI
            };
            assert!((f64::from(outputs[0][i]) / expected - 1.0).abs() < 0.03);
        }
    }
}
