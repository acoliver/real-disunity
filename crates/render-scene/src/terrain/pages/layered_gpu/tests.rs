use super::*;
use crate::terrain::pages::{gpu_tests::read, layered::LayeredReference, residency_tests::context};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::*;

pub(super) fn fixture(
    filter: TerrainSplatFilter,
    channels: TerrainColorizationChannels,
) -> (SceneTerrain, SceneResources) {
    let surfaces = (0..3)
        .map(|i| {
            SceneTerrainSurface::new(
                MaterialHandle::from_raw(0),
                SceneTerrainAppearance {
                    layered: Some(TerrainLayer {
                        albedo_metallic: TextureHandle::from_raw(i * 3),
                        normal_roughness: TextureHandle::from_raw(i * 3 + 1),
                        height: TextureHandle::from_raw(i * 3 + 2),
                        normal_encoding: SceneNormalEncoding::TwoChannelXy,
                        repeat_millimeters: 1700,
                        offset_millimeters: [130, -70],
                        roughness_metallic_offset_micro: [-100_000, 70_000],
                    }),
                    rotation_arcminutes: 1700,
                    blur_radius_millimeters: 1300,
                    tint_percent: [80, 120, 100],
                },
            )
        })
        .collect();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 2000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces,
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        channel_map: None,
        type_grid: TerrainTypeGrid::new([3, 3], [0; 2], 700, vec![0, 1, 2, 1, 2, 0, 2, 0, 1])
            .unwrap(),
        filter,
        colorization: Some(TerrainColorization {
            texture: TextureHandle::from_raw(9),
            channels,
            origin_millimeters: [0; 2],
            extent_millimeters: [2000; 2],
        }),
    })
    .unwrap();
    (terrain, resources(channels))
}
fn resources(channels: TerrainColorizationChannels) -> SceneResources {
    let image = |format, color_space, bytes: Vec<u8>| SceneTexture::Texels {
        format,
        color_space,
        width: 2,
        height: 1,
        mips: 1,
        bytes: bytes.into_boxed_slice(),
    };
    let mut textures = Vec::new();
    for i in 0..3u8 {
        textures.push(image(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Srgb,
            vec![30 + i * 60, 100, 180, 40, 80, 40 + i * 40, 140, 80],
        ));
        textures.push(image(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Linear,
            vec![150, 100, 0, 100, 110, 150, 0, 180],
        ));
        textures.push(image(
            TexelFormat::R8Unorm,
            TextureColorSpace::Linear,
            vec![20 + i * 80, 70 + i * 70],
        ));
    }
    textures.push(image(
        TexelFormat::Rgba8Unorm,
        TextureColorSpace::Linear,
        vec![180, 120, 90, 140, 120, 190, 150, 100],
    ));
    if channels == TerrainColorizationChannels::Grayscale {
        textures[9] = image(
            TexelFormat::R8Unorm,
            TextureColorSpace::Linear,
            vec![180, 120],
        );
    }
    SceneResources::new(textures, vec![]).unwrap()
}
pub(super) fn execute(
    context: &crate::device::GpuContext,
    source: &LayeredGpuSource,
    positions: &[[f32; 2]],
) -> Vec<u8> {
    let device = context.device();
    let mut registry = ResourceRegistry::default();
    let (output, mut operations) = source
        .operations(device, context.queue(), &mut registry, positions)
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
        id("layered test readback"),
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
fn gpu_matches_reference_for_dither_crossover_height_and_colorization() {
    let context = context();
    let positions = [
        [0.1, 0.3],
        [1.0, 1.0],
        [1.25, 0.75],
        [1.75, 1.25],
        [-0.25, -0.125],
        [2.3, 1.7],
    ];
    for filter in [TerrainSplatFilter::Spiral14, TerrainSplatFilter::Bilinear] {
        for channels in [
            TerrainColorizationChannels::Rgb,
            TerrainColorizationChannels::Grayscale,
            TerrainColorizationChannels::BgrRoughness,
        ] {
            let (terrain, resources) = fixture(filter, channels);
            let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
            let reference = LayeredReference::new(&terrain, &resources).unwrap();
            let actual = execute(&context, &source, &positions);
            for (world, actual) in positions.into_iter().zip(actual.chunks_exact(16)) {
                let (material, height) = reference.sample(world).unwrap().pack();
                let expected: Vec<_> = material
                    .color
                    .into_iter()
                    .chain(material.normal_roughness)
                    .chain([height])
                    .collect();
                // Hardware filtering and f32 shader transcendental precision
                // may move the final UNORM rounding by one byte. No sRGB output encoding.
                for (a, e) in actual[..9].iter().zip(expected) {
                    assert!(
                        a.abs_diff(e) <= 1,
                        "{world:?}: actual {actual:?}, expected byte {e}"
                    );
                }
            }
        }
    }
}
#[test]
fn layered_pages_publish_three_products_and_evict_atomically() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = crate::terrain::pages::residency_tests::settings().parts();
    parts.upload_bytes_per_frame = 1008;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let (terrain, resources) = fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let cache = ResidentPages::new_layered(
        device,
        &authority,
        settings,
        2,
        AllocationBudget(100_000),
        &source,
    )
    .unwrap();
    assert_eq!(cache.product_count(), 3);
    assert_eq!(cache.page_bytes(), settings.page_bytes() / 2 * 3);
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    source
        .encode_page(device, &mut frame, &cache, pending)
        .unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_none());
    let serial = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(serial),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let resident = cache.lookup(PageKey::ROOT, 0).unwrap();
    assert!(cache.is_current(resident));
    cache.evict(PageKey::ROOT);
    assert!(!cache.is_current(resident));
}

#[test]
fn layered_admission_includes_height_inputs_and_upload_staging() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let (terrain, resources) = fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let mut parts = crate::terrain::pages::residency_tests::settings().parts();
    parts.upload_bytes_per_frame = 1008;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache = ResidentPages::new_layered(
        device,
        &authority,
        settings,
        2,
        AllocationBudget(100_000),
        &source,
    )
    .unwrap();
    let base = u64::from(settings.extent()[0]) * u64::from(settings.extent()[1]);
    let expected = settings.page_bytes() * 3 + (base * 24 + 8) * 4 + source.input_bytes();
    assert_eq!(cache.allocation_bytes(device).unwrap(), expected);
    assert!(
        ResidentPages::new_layered(
            device,
            &authority,
            settings,
            2,
            AllocationBudget(expected - 1),
            &source
        )
        .is_err()
    );
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    source
        .encode_page(device, &mut frame, &cache, pending)
        .unwrap();
    frame.finish().cancel();
    assert!(cache.lookup(PageKey::ROOT, 0).is_none());
    let mut frame = authority.begin(device).unwrap();
    let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
    source
        .encode_page(device, &mut frame, &cache, pending)
        .unwrap();
    let serial = frame.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(serial),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    assert!(cache.lookup(PageKey::ROOT, 0).is_some());
}
