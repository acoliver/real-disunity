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
    // Compare horizontal pairs of packed RGBA colors in sixteen-byte records.
    bytes
        .chunks_exact(32)
        .map(|pair| u64::from(pair[0].abs_diff(pair[16])).pow(2))
        .sum()
}

#[test]
fn source_anisotropy_preserves_detail_across_the_minor_footprint_axis() {
    let context = crate::terrain::pages::residency_tests::context();
    // Match each SIMD quad's coordinates to the explicit 1x16 texel Jacobian.
    // Software anisotropic filters can also derive their ellipse from lane UVs.
    let positions: Vec<_> = (0_u16..64)
        .map(|lane| {
            [
                0.5 + f32::from(lane / 4 * 2 + lane % 2) / 256.0,
                0.5 + f32::from(lane % 4 / 2) * 16.0 / 256.0,
            ]
        })
        .collect();
    let render = |level, footprint| {
        super::source_mip_tests::execute_footprint(&context, &source(level), &positions, footprint)
    };
    let baseline = energy(&render(1, [1.0 / 256.0; 2]));
    assert!(baseline > 10_000, "small isotropic footprint: {baseline}");
    let one = render(1, [1.0 / 256.0, 16.0 / 256.0]);
    let sixteen = render(16, [1.0 / 256.0, 16.0 / 256.0]);
    // The 1x major axis selects mip four, authored as opaque uniform gray.
    assert!(
        one.chunks_exact(16)
            .all(|record| record[..4] == [128, 128, 128, 255]),
        "coarse isotropic output must contain the authored gray mip"
    );
    let low = energy(&one);
    let high = energy(&sixteen);
    if context
        .adapter()
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING)
    {
        assert!(
            high > low + 10_000,
            "source detail energy: 1x={low}, 16x={high}"
        );
    } else {
        assert_eq!(sixteen, one, "unsupported anisotropy must clamp to 1x");
    }
}
