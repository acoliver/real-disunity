use super::*;
use crate::terrain::pages::{layered::LayeredReference, residency_tests::context};
use worldrunner_render_plan::*;

fn fixture(rotation: u32) -> (SceneTerrain, SceneResources) {
    let (terrain, resources) = super::source_mip_tests::fixture(rotation, 1700, [1; 3]);
    let mut textures = resources.textures().to_vec();
    textures[1] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![255, 128, 128, 100, 128, 255, 128, 180].into_boxed_slice(),
    };
    (terrain, SceneResources::new(textures, vec![]).unwrap())
}

#[test]
fn terrain_large_rotation_cpu_matches_reduced_angle() {
    let (terrain, resources) = fixture(2_160_000_000);
    let (reduced, _) = fixture(0);
    let actual = LayeredReference::new(&terrain, &resources).unwrap();
    let expected = LayeredReference::new(&reduced, &resources).unwrap();
    for world in [[0.1, 0.3], [1.25, 0.75], [8.0, 4.0]] {
        assert_eq!(
            actual.sample(world).unwrap().pack(),
            expected.sample(world).unwrap().pack()
        );
    }
}

#[test]
fn terrain_large_rotation_gpu_matches_cpu_with_one_byte_tolerance() {
    let context = context();
    let (terrain, resources) = fixture(2_160_000_000);
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let reference = LayeredReference::new(&terrain, &resources).unwrap();
    let positions = [[0.1, 0.3], [1.25, 0.75], [8.0, 4.0]];
    let actual = super::tests::execute(&context, &source, &positions);
    for (world, actual) in positions.into_iter().zip(actual.chunks_exact(16)) {
        let (material, height) = reference.sample(world).unwrap().pack();
        let expected = material
            .color
            .into_iter()
            .chain(material.normal_roughness)
            .chain([height]);
        for (actual, expected) in actual[..9].iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 1,
                "{world:?}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn terrain_ordinary_rotations_preserve_gpu_product_bytes() {
    let context = context();
    let positions = [[0.1, 0.3], [1.25, 0.75], [8.0, 4.0]];
    for rotation in [0_u16, 1700, 2700, 5400, 10_800, 21_599] {
        let (terrain, resources) = fixture(u32::from(rotation));
        let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
        let actual = super::tests::execute(&context, &source, &positions);
        let mut previous = LayeredGpuSource::new(&terrain, &resources).unwrap();
        let angle = f32::from(rotation) * std::f32::consts::PI / 10_800.0;
        previous.parameters[112..116].copy_from_slice(&angle.cos().to_le_bytes());
        previous.parameters[116..120].copy_from_slice(&angle.sin().to_le_bytes());
        assert_eq!(
            actual,
            super::tests::execute(&context, &previous, &positions),
            "rotation {rotation}"
        );
    }
}
