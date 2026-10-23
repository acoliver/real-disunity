use super::*;
use crate::shadows::cascades::CascadeViews;

fn fits(context: &GpuContext, source: &RenderScene) -> CascadeViews {
    let renderer = ShadowedScene::new(context, source, settings()).unwrap();
    CascadeViews::new(
        &renderer.gpu,
        &view(),
        source.effects().unwrap(),
        settings(),
        Vec3::new(0.6, 0.0, 0.8),
    )
    .unwrap()
}

fn depth_scale(matrix: glam::Mat4) -> f32 {
    matrix.transpose().z_axis.truncate().length()
}

fn texel_width(matrix: glam::Mat4) -> f32 {
    2.0 / (matrix.transpose().x_axis.truncate().length() * 512.0)
}

#[test]
fn cascade_bias_preserves_reference_texels_across_box_and_sphere_fits_and_casters() {
    let context = context();
    let policies = [
        SceneShadowCascades::new(vec![1_000, 2_000, 4_000, 8_000], 0).unwrap(),
        SceneShadowCascades::camera_splits(vec![2_000, 4_000, 8_000, 20_000], 100_000).unwrap(),
    ];
    for policy in policies {
        let base = super::cascades::source_with(&scene(&[], 600_000, None), policy);
        let receiver = fits(&context, &base);
        let extended = super::cascades::source_with(
            &scene(
                &[[600_000, 3_000, 800_500], [1_200_000, 6_000, 1_600_500]],
                600_000,
                None,
            ),
            base.effects().unwrap().shadows().clone(),
        );
        let caster = fits(&context, &extended);
        let reference_texels = settings().depth_bias * settings().depth_half_extent_meters * 512.0
            / settings().half_extent_meters;
        for index in 0..4 {
            let before = receiver.matrices[index];
            let after = caster.matrices[index];
            assert!((texel_width(before) - texel_width(after)).abs() < 1e-6);
            for fit in [&receiver, &caster] {
                let matrix = fit.matrices[index];
                let actual_texels =
                    fit.depth_bias[index] / depth_scale(matrix) / texel_width(matrix);
                assert!(
                    (actual_texels - reference_texels).abs() < 1e-5,
                    "layer {index}: {actual_texels}, expected {reference_texels}"
                );
            }
            let before_world = receiver.depth_bias[index] / depth_scale(before);
            let after_world = caster.depth_bias[index] / depth_scale(after);
            assert!((before_world - after_world).abs() < 1e-6);
        }
        assert_eq!(receiver.matrices[0], caster.matrices[0]);
        assert!(depth_scale(caster.matrices[2]) < depth_scale(receiver.matrices[2]));
        assert!(depth_scale(caster.matrices[3]) < depth_scale(caster.matrices[2]));
        assert!(texel_width(receiver.matrices[3]) > texel_width(receiver.matrices[0]));
        let bytes = resources::uniform_bytes(&caster, settings());
        assert_eq!(bytes.len(), 336);
        for (encoded, expected) in bytes[320..].chunks_exact(4).zip(caster.depth_bias) {
            assert_eq!(encoded, expected.to_le_bytes());
        }
    }
}

#[test]
fn gpu_distant_non_occluding_caster_does_not_erase_nearby_cascade_shadow() {
    let context = context();
    let policy = SceneShadowCascades::new(vec![1_000, 2_000], 0).unwrap();
    let source = super::cascades::source_with(&scene(&[[0, 1_700, 100]], 600_000, None), policy);
    let extended = super::cascades::source_with(
        &scene(
            &[[0, 1_700, 100], [600_000, -1_700, 800_000]],
            600_000,
            None,
        ),
        source.effects().unwrap().shadows().clone(),
    );
    let before = fits(&context, &source);
    let after = fits(&context, &extended);
    assert_eq!(before.matrices[0], after.matrices[0]);
    assert!(depth_scale(after.matrices[1]) < depth_scale(before.matrices[1]) / 100.0);
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let point = Vec3::new(-0.34, 1.7, 0.0);
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    assert_shadow(&image, &baseline, point);
    let distant = capture_scene_shadowed(&context, &extended, &view(), settings()).unwrap();
    assert_shadow(&distant, &baseline, point);
    assert!(at(&image, point).abs_diff(at(&distant, point)) <= 2);
}
