use super::*;
use crate::sampling::tests::checker;
use worldrunner_render_plan::{SceneResources, TextureAnisotropy};

fn source(level: u16) -> LayeredGpuSource {
    let (terrain, resources) = super::source_mip_tests::fixture(0, 1000, [4; 3]);
    let mut textures = resources.textures().to_vec();
    textures[0] = checker();
    let resources = SceneResources::new(textures, vec![]).unwrap();
    let mut authored = terrain.layered_source().unwrap().clone();
    authored.anisotropy = TextureAnisotropy::new(level).unwrap();
    let terrain = terrain.with_layered_source(authored).unwrap();
    LayeredGpuSource::new(&terrain, &resources).unwrap()
}

fn energy(bytes: &[u8]) -> u64 {
    // Composition writes one packed RGBA color in each sixteen-byte output record.
    bytes
        .chunks_exact(16)
        .map(|record| record[0])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| u64::from(pair[0].abs_diff(pair[1])).pow(2))
        .sum()
}

#[test]
fn source_anisotropy_preserves_detail_across_the_minor_footprint_axis() {
    let context = crate::terrain::pages::residency_tests::context();
    // One-metre repeats and 256 texels give independently known 1x16 texel axes.
    let positions: Vec<_> = (0_u16..64)
        .map(|x| [0.5 + f32::from(x) / 256.0, 0.5])
        .collect();
    let render = |level| {
        super::source_mip_tests::execute_footprint(
            &context,
            &source(level),
            &positions,
            [1.0 / 256.0, 16.0 / 256.0],
        )
    };
    let low = energy(&render(1));
    let high = energy(&render(16));
    assert!(
        high > low + 10_000,
        "source detail energy: 1x={low}, 16x={high}"
    );
}
