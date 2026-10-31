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
    assert!(
        high > low + 10_000,
        "source detail energy: 1x={low}, 16x={high}; {diagnostic}"
    );
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

#[test]
fn source_sampling_matches_independent_cpu_taps() {
    let context = crate::terrain::pages::residency_tests::context();
    let reference = super::anisotropy_reference::Reference::new();
    let points = [[0.125, 0.25], [-0.375, 0.625], [1.25, -0.125]];
    for (level, rotation) in [2, 4, 8, 16]
        .into_iter()
        .flat_map(|level| [0, 2700, 5400].map(|angle| (level, angle)))
    {
        let source = tiny_rotated_source(level, &reference, rotation);
        let resident = source.resident(context.device(), context.queue()).unwrap();
        assert!(
            resident
                .sampler_descriptors
                .iter()
                .all(|s| s.anisotropy_clamp == 1)
        );
        for footprint in [
            [0.0; 2],
            [0.125, 0.25],
            [0.5, 0.125],
            [0.375; 2],
            [0.3125, 0.375],
            [0.0, 0.625],
            [2.0, 0.25],
            [4.0; 2],
        ] {
            let bytes =
                super::source_mip_tests::execute_footprint(&context, &source, &points, footprint);
            check_cpu(
                &context,
                &reference,
                &points,
                &bytes,
                (footprint, level, rotation),
            );
        }
    }
}

fn check_cpu(
    context: &crate::device::GpuContext,
    reference: &super::anisotropy_reference::Reference,
    points: &[[f32; 2]],
    bytes: &[u8],
    case: ([f32; 2], u16, u32),
) {
    let (footprint, level, angle) = case;
    // Use the authored f32 rotation coefficients as inputs to the independent
    // f64 filter, so exact ratio-one footprints stay ratio one in both paths.
    let radians = f32::from(u16::try_from(angle).unwrap()) * std::f32::consts::PI / 10_800.0;
    let authored = glam::Mat2::from_angle(radians);
    let rotation = glam::DMat2::from_cols(authored.x_axis.as_dvec2(), authored.y_axis.as_dvec2());
    let gradients = [
        rotation * glam::DVec2::new(f64::from(footprint[0]), 0.0),
        rotation * glam::DVec2::new(0.0, f64::from(footprint[1])),
    ];
    for (point, record) in points.iter().zip(bytes.chunks_exact(16)) {
        let uv = rotation * glam::DVec2::from_array(point.map(f64::from))
            + glam::DVec2::new(0.13, -0.07);
        let expected = reference.sample(uv, gradients, level);
        // Three 8-bit filter-weight roundings over a 180-value range,
        // plus final byte packing: 3 * 180 / 256 + 0.5 < 3 bytes.
        assert!(
            record[..3]
                .iter()
                .all(|actual| (f64::from(*actual) - expected).abs() <= 3.0),
            "adapter={:?}, level={level}, rotation={angle}, point={point:?}, gradients={gradients:?}, GPU={record:?}, CPU={expected}",
            context.adapter().get_info()
        );
    }
}

fn tiny_source(level: u16, reference: &super::anisotropy_reference::Reference) -> LayeredGpuSource {
    tiny_rotated_source(level, reference, 0)
}

fn tiny_rotated_source(
    level: u16,
    reference: &super::anisotropy_reference::Reference,
    rotation: u32,
) -> LayeredGpuSource {
    let (terrain, _) = super::source_mip_tests::fixture(rotation, 1000, [4; 3]);
    let textures = vec![
        reference.texture(),
        reference.constant_texture([128, 128, 255, 30]),
        reference.constant_texture([32; 4]),
    ];
    let resources = SceneResources::new(textures, vec![]).unwrap();
    let mut authored = terrain.layered_source().unwrap().clone();
    authored.anisotropy = TextureAnisotropy::new(level).unwrap();
    LayeredGpuSource::new(&terrain.with_layered_source(authored).unwrap(), &resources).unwrap()
}

#[test]
fn source_ratio_one_matches_isotropic_bytes() {
    let context = crate::terrain::pages::residency_tests::context();
    let reference = super::anisotropy_reference::Reference::new();
    let points = [[0.125, 0.25], [-0.375, 0.625], [1.25, -0.125]];
    let one = tiny_source(1, &reference);
    for footprint in [[0.125, 0.25], [0.25, 0.5], [0.5, 1.0]] {
        let expected =
            super::source_mip_tests::execute_footprint(&context, &one, &points, footprint);
        for level in [2, 4, 8, 16] {
            let source = tiny_source(level, &reference);
            assert_eq!(
                super::source_mip_tests::execute_footprint(&context, &source, &points, footprint),
                expected,
                "ratio=1, level={level}, footprint={footprint:?}, adapter={:?}",
                context.adapter().get_info()
            );
        }
    }
}

#[test]
fn source_one_x_matches_main_bytes() {
    let context = crate::terrain::pages::residency_tests::context();
    let reference = super::anisotropy_reference::Reference::new();
    let source = tiny_source(1, &reference);
    let resident = source.resident(context.device(), context.queue()).unwrap();
    assert!(
        resident
            .sampler_descriptors
            .iter()
            .all(|s| s.anisotropy_clamp == 1)
    );
    let points = [[0.125, 0.25], [-0.375, 0.625], [1.25, -0.125]];
    // The sampling function is copied verbatim from main a906e89. Running it
    // on the same adapter avoids treating backend rounding as a default change.
    let shader = LAYERED_WGSL.replace("fn layered_sample(", "fn unused_layered_sample(")
        + include_str!("main_sample_test.wgsl");
    for footprint in [[0.0; 2], [0.125, 0.25], [0.5, 0.125], [4.0; 2]] {
        let expected = super::source_mip_tests::execute_shader(
            &context,
            &source,
            &points,
            footprint,
            Some(&shader),
        );
        assert_eq!(
            super::source_mip_tests::execute_footprint(&context, &source, &points, footprint),
            expected,
            "1x differs from main a906e89: footprint={footprint:?}, adapter={:?}",
            context.adapter().get_info()
        );
    }
}

pub(super) fn replace_pipeline(
    device: &wgpu::Device,
    operations: &mut [TypedOperation],
    shader: &str,
) {
    let TypedWork::Compute {
        pipeline, bindings, ..
    } = &mut operations.last_mut().unwrap().work
    else {
        panic!("composition must end with compute");
    };
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(bindings[0].layout())],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("main a906e89 sampling reference"),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    *pipeline = worldrunner_render_post::compute_pipeline(
        device,
        worldrunner_render_post::ComputeDescriptor {
            label: Some("main sampling reference"),
            layout: &layout,
            module: &module,
            entry: "bake_layered",
            constants: &[],
        },
    );
}
