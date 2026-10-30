use super::*;
use crate::terrain::pages::{
    gpu_tests::read,
    layered::{HeightLayerSample, LayeredReference, compose_height_layers},
    residency_tests::context,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::*;

const COLORS: [[u8; 4]; 4] = [
    [24, 40, 80, 20],
    [90, 110, 140, 70],
    [170, 130, 80, 160],
    [240, 210, 180, 240],
];
const NORMALS: [[u8; 4]; 4] = [
    [128, 128, 255, 30],
    [160, 128, 255, 90],
    [128, 160, 255, 160],
    [100, 100, 255, 230],
];
const HEIGHTS: [[u8; 4]; 4] = [[32; 4], [90; 4], [160; 4], [240; 4]];
const SHAPES: [[u32; 2]; 3] = [[9, 5], [5, 9], [9, 2]];

fn texture(index: usize, mips: u32) -> SceneTexture {
    let [width, height] = SHAPES[index];
    let levels = [COLORS, NORMALS, HEIGHTS][index];
    let channels = if index == 2 { 1 } else { 4 };
    let bytes: Vec<_> = (0..mips)
        .flat_map(|level| {
            let value = levels[usize::try_from(level).unwrap()];
            let count = (width >> level).max(1) * (height >> level).max(1);
            (0..count).flat_map(move |_| value[..channels].to_vec())
        })
        .collect();
    SceneTexture::Texels {
        format: if index == 2 {
            TexelFormat::R8Unorm
        } else {
            TexelFormat::Rgba8Unorm
        },
        color_space: if index == 0 {
            TextureColorSpace::Srgb
        } else {
            TextureColorSpace::Linear
        },
        width,
        height,
        mips,
        bytes: bytes.into_boxed_slice(),
    }
}

pub(super) fn fixture(
    rotation: u32,
    repeat: u32,
    counts: [u32; 3],
) -> (SceneTerrain, SceneResources) {
    let surface = SceneTerrainSurface::new(
        MaterialHandle::from_raw(0),
        SceneTerrainAppearance {
            layered: Some(TerrainLayer {
                albedo_metallic: TextureHandle::from_raw(0),
                normal_roughness: TextureHandle::from_raw(1),
                height: TextureHandle::from_raw(2),
                normal_encoding: SceneNormalEncoding::TwoChannelXy,
                repeat_millimeters: repeat,
                offset_millimeters: [130, -70],
                roughness_metallic_offset_micro: [0; 2],
            }),
            rotation_arcminutes: rotation,
            blur_radius_millimeters: 0,
            tint_percent: [100; 3],
        },
    );
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 12_000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![surface],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        anisotropy: worldrunner_render_plan::TextureAnisotropy::default(),
        channel_map: None,
        type_grid: TerrainTypeGrid::new([2; 2], [0; 2], 12_000, vec![0; 4]).unwrap(),
        filter: TerrainSplatFilter::Spiral14,
        colorization: None,
    })
    .unwrap();
    let textures = counts
        .into_iter()
        .enumerate()
        .map(|(i, count)| texture(i, count))
        .collect();
    (terrain, SceneResources::new(textures, vec![]).unwrap())
}

// Derive the two basis-vector lengths in f64 independently of the shader's
// coordinate transform, then interpolate the authored constant levels.
fn oracle_weights(shape: [u32; 2], footprint: [f64; 2], rotation: u32, repeat: u32) -> [f64; 4] {
    let angle = f64::from(rotation) * std::f64::consts::PI / 10_800.0;
    let [w, h] = shape.map(f64::from);
    let east = footprint[0] * (w * angle.cos()).hypot(h * angle.sin());
    let north = footprint[1] * (w * angle.sin()).hypot(h * angle.cos());
    let ratio = east.max(north) / (f64::from(repeat) / 1000.0);
    let lod = ratio.max(1.0).log2().clamp(0.0, 3.0);
    std::array::from_fn(|level| {
        (1.0 - (lod - f64::from(u32::try_from(level).unwrap())).abs()).max(0.0)
    })
}

fn oracle_product(index: usize, weights: [f64; 4]) -> [f64; 4] {
    let levels = [COLORS, NORMALS, HEIGHTS][index];
    std::array::from_fn(|channel| {
        levels
            .iter()
            .zip(weights)
            .map(|(pixel, weight)| {
                let value = f64::from(pixel[channel]) / 255.0;
                let linear = if index == 0 && channel < 3 {
                    if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
                } else {
                    value
                };
                linear * weight
            })
            .sum()
    })
}

fn oracle_texel(footprint: [f64; 2], rotation: u32, repeat: u32) -> Vec<u8> {
    let products: [[f64; 4]; 3] = std::array::from_fn(|index| {
        oracle_product(
            index,
            oracle_weights(SHAPES[index], footprint, rotation, repeat),
        )
    });
    let color = glam::DVec4::from_array(products[0]).as_vec4().to_array();
    let packed = glam::DVec4::from_array(products[1]).as_vec4().to_array();
    let xy = glam::Vec2::new(packed[0], packed[1]) * 2.0 - glam::Vec2::ONE;
    let sample = HeightLayerSample {
        albedo: [color[0], color[1], color[2]],
        metallic: color[3],
        normal: [xy.x, xy.y, (1.0 - xy.length_squared()).max(0.0).sqrt()],
        roughness: packed[3],
        height: glam::DVec4::from_array(products[2]).as_vec4().x,
        coverage: 1.0,
    };
    let (material, height) =
        compose_height_layers(&[sample], TerrainSplatFilter::Spiral14, [1.0; 3], 0.0)
            .unwrap()
            .pack();
    material
        .color
        .into_iter()
        .chain(material.normal_roughness)
        .chain([height])
        .collect()
}

#[test]
fn independent_level_oracle_covers_fine_coarse_rotation_and_scale() {
    let cases = [
        ([0.001; 2], 0, [1.0, 0.0, 0.0, 0.0]),
        ([10.0; 2], 0, [0.0, 0.0, 0.0, 1.0]),
        ([0.5, 0.125], 0, [0.0, 0.0, 1.0, 0.0]),
        ([0.5, 0.125], 5400, [0.0, 1.0, 0.0, 0.0]),
    ];
    for (footprint, angle, expected) in cases {
        let actual = oracle_weights([8, 4], footprint, angle, 1000);
        assert!(
            actual
                .into_iter()
                .zip(expected)
                .all(|(a, e)| (a - e).abs() < 1e-12)
        );
    }
    let weights = oracle_weights([8, 4], [0.5, 0.125], 2700, 2000);
    let expected = (10.0_f64.sqrt() / 2.0).log2();
    assert!((weights[1] - expected).abs() < 1e-12);
    assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1e-12);
}

#[test]
fn composed_pages_match_independent_trilinear_oracle() {
    let context = context();
    for (rotation, repeat) in [(0, 750), (2700, 1700), (5400, 9000)] {
        let (terrain, resources) = fixture(rotation, repeat, [4; 3]);
        let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
        let reference = LayeredReference::new(&terrain, &resources).unwrap();
        check_pages(&context, &source, &reference, rotation, repeat);
    }
}

fn check_pages(
    context: &crate::device::GpuContext,
    source: &LayeredGpuSource,
    reference: &LayeredReference<'_>,
    rotation: u32,
    repeat: u32,
) {
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = crate::terrain::pages::residency_tests::settings().parts();
    parts.upload_bytes_per_frame = 1008;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache = ResidentPages::new_layered(
        device,
        &authority,
        settings,
        1,
        AllocationBudget(100_000),
        source,
    )
    .unwrap();
    check_budget(device, &authority, settings, source, &cache);
    let mut scratch = LayeredScratch::new(device, source, &cache).unwrap();
    for depth in 0..=3 {
        let key = PageKey::new(depth, [0; 2]).unwrap();
        let footprint = [12.0 / 3.0, 12.0 / 5.0].map(|v| v / f64::from(1_u32 << depth));
        let expected = oracle_texel(footprint, rotation, repeat);
        let cpu = reference.compose_page(settings, key).unwrap();
        for texel in cpu {
            let (material, height) = texel.pack();
            let bytes = material
                .color
                .into_iter()
                .chain(material.normal_roughness)
                .chain([height]);
            assert!(bytes.zip(&expected).all(|(a, e)| a.abs_diff(*e) <= 1));
        }
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, key).unwrap();
        let slot = pending.slot();
        if depth % 2 == 0 {
            source
                .encode_page(device, &mut frame, &cache, pending)
                .unwrap();
        } else {
            scratch
                .encode(source, device, &mut frame, &cache, pending)
                .unwrap();
        }
        let buffer =
            super::mip_tests::copy(device, &mut frame, &cache, [0, slot], settings.extent());
        frame.finish().submit().unwrap();
        let actual = super::mip_tests::unpack(device, &buffer, settings.extent());
        for texel in actual {
            for (channel, (actual, expected)) in texel.into_iter().zip(&expected).enumerate() {
                assert!(
                    actual.abs_diff(*expected) <= 1,
                    "depth {depth}, rotation {rotation}, repeat {repeat}, channel {channel}: {actual} != {expected}"
                );
            }
        }
        cache.evict(key);
    }
}

#[test]
fn incomplete_authored_chains_are_typed_errors_for_every_product() {
    for index in 0..3 {
        let mut counts = [4; 3];
        counts[index] = 2;
        let (terrain, resources) = fixture(0, 1000, counts);
        assert!(
            matches!(LayeredGpuSource::new(&terrain, &resources), Err(LayeredError::IncompleteMipChain { declared: 2, expected: 4, texture }) if texture == u32::try_from(index).unwrap())
        );
    }
}

#[test]
fn every_authored_mip_is_charged_to_source_memory() {
    let (terrain, resources) = fixture(0, 1000, [4; 3]);
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let texels: u64 = SHAPES
        .into_iter()
        .map(|[w, h]| {
            (0..4)
                .map(|level| u64::from((w >> level).max(1) * (h >> level).max(1)))
                .sum::<u64>()
        })
        .sum();
    let height_texels: u64 = (0..4)
        .map(|level| u64::from((9_u32 >> level).max(1) * (2_u32 >> level).max(1)))
        .sum();
    assert_eq!(
        source.input_bytes(),
        128 + 3 * 16 + 4 * 4 + texels * 4 - height_texels * 3
    );
    assert_eq!(source.grid.len(), 4 * 4);
}

#[test]
fn single_mip_gpu_products_are_byte_identical_for_all_footprints() {
    let context = context();
    let (terrain, resources) = super::tests::fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let positions = [[0.1, 0.3], [1.25, 0.75], [-0.25, -0.125]];
    let original = super::tests::execute(&context, &source, &positions);
    for footprint in [[0.001; 2], [1.0, 4.0], [1024.0; 2]] {
        assert_eq!(
            execute_footprint(&context, &source, &positions, footprint),
            original
        );
    }
}

pub(super) fn execute_footprint(
    context: &crate::device::GpuContext,
    source: &LayeredGpuSource,
    positions: &[[f32; 2]],
    footprint: [f32; 2],
) -> Vec<u8> {
    let device = context.device();
    let mut registry = ResourceRegistry::default();
    let (output, mut operations) = source
        .operations_for(
            device,
            context.queue(),
            &mut registry,
            (positions, footprint),
        )
        .unwrap();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: positions.len() as u64 * 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let key = registry
        .import_buffer(&buffer, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let destination = registry.buffer_view(key, 0..buffer.size(), 1).unwrap();
    operations.push(TypedOperation::new(
        id("mip test readback"),
        TypedWork::ReadbackBuffer {
            source: output,
            destination,
        },
    ));
    let compiled = registry.compile_operations(device, &operations).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    context.queue().submit([encoder.finish()]);
    read(device, &buffer)
}

#[test]
fn coarse_authored_heights_drive_the_same_crossover_as_explicit_last_levels() {
    let context = context();
    for filter in [TerrainSplatFilter::Spiral14, TerrainSplatFilter::Bilinear] {
        let (terrain, base) =
            super::tests::fixture(filter, TerrainColorizationChannels::BgrRoughness);
        let (authored, last) = crossover_resources(&base);
        let source = LayeredGpuSource::new(&terrain, &authored).unwrap();
        let explicit = LayeredGpuSource::new(&terrain, &last).unwrap();
        let positions = [[0.1, 0.3], [1.0, 1.0], [1.25, 0.75], [1.75, 1.25]];
        let expected = super::tests::execute(&context, &explicit, &positions);
        let actual = execute_footprint(&context, &source, &positions, [1024.0; 2]);
        assert_eq!(actual, expected);
    }
}

fn crossover_resources(base: &SceneResources) -> (SceneResources, SceneResources) {
    let pairs: Vec<_> = base
        .textures()
        .iter()
        .enumerate()
        .map(|(index, texture)| {
            if index == 9 {
                return (texture.clone(), texture.clone());
            }
            let index = u8::try_from(index).unwrap();
            let pixel = match index % 3 {
                0 => vec![200 - index * 10, 40 + index * 10, 70, 180],
                1 => vec![100 + index * 4, 170, 0, 30],
                _ => vec![230 - index * 20],
            };
            let SceneTexture::Texels {
                format,
                color_space,
                width,
                height,
                bytes,
                ..
            } = texture
            else {
                unreachable!()
            };
            let authored = SceneTexture::Texels {
                format: *format,
                color_space: *color_space,
                width: *width,
                height: *height,
                mips: 2,
                bytes: [bytes.as_ref(), pixel.as_slice()]
                    .concat()
                    .into_boxed_slice(),
            };
            let last = SceneTexture::Texels {
                format: *format,
                color_space: *color_space,
                width: 1,
                height: 1,
                mips: 1,
                bytes: pixel.into_boxed_slice(),
            };
            (authored, last)
        })
        .collect();
    let (authored, last) = pairs.into_iter().unzip();
    (
        SceneResources::new(authored, vec![]).unwrap(),
        SceneResources::new(last, vec![]).unwrap(),
    )
}

fn check_budget(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    settings: TerrainPageSettings,
    source: &LayeredGpuSource,
    cache: &ResidentPages,
) {
    let [w, h] = settings.extent();
    let page = settings.page_bytes() / 2 * 3;
    let scratch = u64::from(w) * u64::from(h) * 24 + 8;
    let expected = page + scratch * 4 + source.input_bytes();
    assert_eq!(cache.allocation_bytes(device).unwrap(), expected);
    assert!(
        ResidentPages::new_layered(
            device,
            authority,
            settings,
            1,
            AllocationBudget(expected - 1),
            source
        )
        .is_err()
    );
}

#[test]
fn sampler_wrappers_preserve_every_level_and_incomplete_chain_diagnostics() {
    for counts in [[4; 3], [4, 2, 4]] {
        let (terrain, resources) = fixture(2700, 1700, counts);
        let textures = resources
            .textures()
            .iter()
            .cloned()
            .map(|texture| texture.with_sampler(TextureSampler::default()))
            .collect();
        let sampled = SceneResources::new(textures, vec![]).unwrap();
        if counts == [4; 3] {
            let expected = LayeredGpuSource::new(&terrain, &resources).unwrap();
            let actual = LayeredGpuSource::new(&terrain, &sampled).unwrap();
            assert_eq!(actual.input_bytes(), expected.input_bytes());
            assert_eq!(actual.images, expected.images);
        } else {
            assert_eq!(
                LayeredGpuSource::new(&terrain, &sampled).unwrap_err(),
                LayeredError::IncompleteMipChain {
                    texture: 1,
                    declared: 2,
                    expected: 4
                }
            );
        }
    }
}
