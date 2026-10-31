use super::anisotropy_reference::Geometry;
use super::{LayeredGpuSource, anisotropy_reference::Reference, source_mip_tests};
use crate::terrain::pages::layered::LayeredReference;
use glam::DVec2;
use worldrunner_render_plan::{SceneResources, SceneTerrain, SceneTexture, TextureAnisotropy};

#[test]
fn analytical_principal_axes_match_rotated_rectangle_and_nonorthogonal_gradients() {
    let diagonal = 0.375 / 2.0_f64.sqrt();
    let rectangular = Geometry::new([DVec2::splat(diagonal), DVec2::new(-diagonal, diagonal)], 2);
    assert!((rectangular.major - 3.0).abs() < 1e-12);
    assert!((rectangular.minor - 1.5).abs() < 1e-12);
    assert!((rectangular.direction - DVec2::new(3.0 / 8.0, 0.0)).length() < 1e-12);
    assert_eq!(rectangular.count.to_bits(), 2.0_f64.to_bits());
    assert!((rectangular.lod - 1.5_f64.log2()).abs() < 1e-12);
    let grazing = Geometry::new([DVec2::new(2.0, 0.25), DVec2::new(2.0, -0.25)], 16);
    assert!((grazing.major - 16.0 * 2.0_f64.sqrt()).abs() < 1e-12);
    assert!((grazing.minor - 2.0_f64.sqrt()).abs() < 1e-12);
    assert!((grazing.major / grazing.minor - 16.0).abs() < 1e-12);
    assert!((grazing.direction - DVec2::new(2.0 * 2.0_f64.sqrt(), 0.0)).length() < 1e-12);
    assert_eq!(grazing.count.to_bits(), 16.0_f64.to_bits());
    assert!((grazing.lod - 0.5).abs() < 1e-12);
    for gradients in [[DVec2::ZERO; 2], [DVec2::X, DVec2::ZERO]] {
        let geometry = Geometry::new(gradients, 16);
        assert!(geometry.lod.is_finite() && geometry.direction.is_finite());
        assert!((1.0..=16.0).contains(&geometry.count));
    }
}

#[test]
fn gpu_matches_analytical_principal_filter_expectations() {
    let context = crate::terrain::pages::residency_tests::context();
    let points = [[0.125, 0.25], [-0.375, 0.625], [1.25, -0.125]];
    let reference = Reference::new();
    for (level, rotation, major, count, lod) in [
        (2, 2700, 3.0 / 8.0, 2.0, 1.5_f64.log2()),
        (16, 0, 2.0 * 2.0_f64.sqrt(), 16.0, 0.5),
    ] {
        let (terrain, resources) = fixture(level, false, rotation);
        let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
        let shader = super::LAYERED_WGSL.replace(
            "vec2<f32>(layered_positions.footprint.x, 0.0), vec2<f32>(0.0, layered_positions.footprint.y)",
            "vec2<f32>(2.0, 0.25), vec2<f32>(2.0, -0.25)");
        let bytes = source_mip_tests::execute_shader(
            &context,
            &source,
            &points,
            [0.375; 2],
            (rotation == 0).then_some(shader.as_str()),
        );
        for (point, record) in points.iter().zip(bytes.chunks_exact(16)) {
            let angle = f64::from(rotation) * std::f64::consts::PI / 10_800.0;
            let uv = glam::DMat2::from_angle(angle) * DVec2::from_array(point.map(f64::from))
                + DVec2::new(0.13, -0.07);
            let expected = reference.sample_geometry(uv, DVec2::new(major, 0.0), count, lod);
            assert!(
                record[..3]
                    .iter()
                    .all(|v| (f64::from(*v) - expected).abs() <= 3.0),
                "level={level}, point={point:?}, GPU={record:?}, analytic={expected}"
            );
        }
    }
}

pub(super) fn fixture(
    level: u16,
    single_mip: bool,
    rotation: u32,
) -> (SceneTerrain, SceneResources) {
    let reference = Reference::new();
    let (terrain, _) = source_mip_tests::fixture(rotation, 1000, [4; 3]);
    let mut textures = vec![
        reference.texture(),
        reference.constant_texture([128, 128, 255, 30]),
        reference.constant_texture([32; 4]),
    ];
    if single_mip {
        for texture in &mut textures {
            let SceneTexture::Texels { mips, bytes, .. } = texture else {
                unreachable!()
            };
            *mips = 1;
            *bytes = bytes[..8 * 4 * 4].into();
        }
    }
    let mut source = terrain.layered_source().unwrap().clone();
    source.anisotropy = TextureAnisotropy::new(level).unwrap();
    (
        terrain.with_layered_source(source).unwrap(),
        SceneResources::new(textures, vec![]).unwrap(),
    )
}

#[test]
fn public_reference_matches_anisotropic_gpu_with_and_without_mips() {
    let context = crate::terrain::pages::residency_tests::context();
    let points = [[0.125, 0.25], [-0.375, 0.625], [1.25, -0.125]];
    for (level, single_mip) in [2, 4, 16]
        .into_iter()
        .flat_map(|level| [false, true].map(|single| (level, single)))
    {
        let (terrain, resources) = fixture(level, single_mip, 2700);
        let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
        let reference = LayeredReference::new(&terrain, &resources).unwrap();
        for footprint in [
            [0.375; 2],
            [2.0, 0.25],
            [0.0; 2],
            [0.0, 0.625],
            [1e-13, 0.0],
        ] {
            let actual = source_mip_tests::execute_footprint(&context, &source, &points, footprint);
            for (point, record) in points.iter().zip(actual.chunks_exact(16)) {
                let (material, height) = reference
                    .sample_with_footprint(*point, footprint)
                    .unwrap()
                    .pack();
                let expected = material
                    .color
                    .into_iter()
                    .chain(material.normal_roughness)
                    .chain([height]);
                assert!(
                    record.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 3),
                    "level={level}, single={single_mip}, point={point:?}, footprint={footprint:?}, GPU={record:?}"
                );
            }
        }
    }
}
