use super::*;
use crate::sampling::tests::checker;
use worldrunner_render_plan::{SceneResources, TextureAnisotropy};

const GRAZING: [f32; 2] = [1.0 / 256.0, 16.0 / 256.0];
const SMALL: [f32; 2] = [1.0 / 256.0; 2];

#[derive(Clone, Copy, Debug)]
enum Shape {
    Row,
    Quads,
}

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

fn positions(shape: Shape) -> Vec<[f32; 2]> {
    (0_u16..64)
        .map(|lane| match shape {
            Shape::Row => [0.5 + f32::from(lane) / 256.0, 0.5],
            // Preserve the neighboring lane coordinates of the quad regression.
            Shape::Quads => [
                0.5 + f32::from(lane / 4 * 2 + lane % 2) / 256.0,
                0.5 + f32::from(lane % 4 / 2) * 16.0 / 256.0,
            ],
        })
        .collect()
}

fn energy(bytes: &[u8], shape: Shape) -> u64 {
    let values: Vec<_> = bytes.chunks_exact(16).map(|record| record[0]).collect();
    match shape {
        Shape::Row => values.windows(2).map(pair_energy).sum(),
        Shape::Quads => values.chunks_exact(2).map(pair_energy).sum(),
    }
}

fn pair_energy(pair: &[u8]) -> u64 {
    u64::from(pair[0].abs_diff(pair[1])).pow(2)
}

#[test]
fn source_anisotropy_preserves_detail_across_the_minor_footprint_axis() {
    check_anisotropy(Shape::Quads);
}

#[test]
fn source_anisotropy_preserves_detail_in_explicit_gradient_row() {
    check_anisotropy(Shape::Row);
}

fn check_anisotropy(shape: Shape) {
    let context = crate::terrain::pages::residency_tests::context();
    let points = positions(shape);
    let sources = [source(1), source(16)];
    let render = |index, footprint| {
        super::source_mip_tests::execute_footprint(&context, &sources[index], &points, footprint)
    };
    let baseline = render(0, SMALL);
    let one = render(0, GRAZING);
    let sixteen = render(1, GRAZING);
    let diagnostic = super::anisotropy_diagnostics::report(
        &context,
        &sources,
        &points,
        [&one, &sixteen, &baseline],
    );
    let low = energy(&one, shape);
    let high = energy(&sixteen, shape);
    eprintln!(
        "{shape:?}: adapter={:?}, source detail energy: 1x={low}, 16x={high}, baseline={}",
        context.adapter().get_info(),
        energy(&baseline, shape)
    );
    assert!(
        energy(&baseline, shape) > 10_000,
        "small isotropic footprint; {diagnostic}"
    );
    if matches!(shape, Shape::Quads) {
        assert!(
            one.chunks_exact(16)
                .all(|record| record[..4] == [128, 128, 128, 255]),
            "coarse isotropic output must contain the authored gray mip; {diagnostic}"
        );
    }
    if matches!(shape, Shape::Row) || anisotropic(&context) {
        assert!(
            high > low + 10_000,
            "source detail energy: 1x={low}, 16x={high}; {diagnostic}"
        );
    } else {
        assert!(
            sixteen == one,
            "unsupported anisotropy must clamp to 1x; {diagnostic}"
        );
    }
}

fn anisotropic(context: &crate::device::GpuContext) -> bool {
    context
        .adapter()
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING)
}

#[test]
fn source_isotropic_small_footprint_row_is_not_flat() {
    check_baseline(Shape::Row);
}

#[test]
fn source_isotropic_small_footprint_quads_are_not_flat() {
    check_baseline(Shape::Quads);
}

fn check_baseline(shape: Shape) {
    let context = crate::terrain::pages::residency_tests::context();
    let points = positions(shape);
    let sources = [source(1)];
    let baseline =
        super::source_mip_tests::execute_footprint(&context, &sources[0], &points, SMALL);
    let detail = energy(&baseline, shape);
    assert!(
        detail > 10_000,
        "source path is flat regardless of anisotropy: shape={shape:?}, baseline energy={detail}; {}",
        super::anisotropy_diagnostics::report(&context, &sources, &points, [&[], &[], &baseline])
    );
}
