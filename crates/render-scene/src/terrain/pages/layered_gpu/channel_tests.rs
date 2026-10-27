use super::*;
use crate::terrain::pages::{layered::LayeredReference, residency_tests::context};
use worldrunner_render_plan::*;

fn channel(texture: TerrainLayerTexture, channel: u8) -> TerrainLayerChannel {
    TerrainLayerChannel { texture, channel }
}

fn consumer_map() -> TerrainLayerChannelMap {
    use TerrainLayerTexture::{AlbedoMetallic, Height, NormalRoughness};
    TerrainLayerChannelMap {
        albedo: [0, 1, 2].map(|c| channel(AlbedoMetallic, c)),
        albedo_color_space: TextureColorSpace::Srgb,
        metallic: channel(Height, 2),
        height: channel(AlbedoMetallic, 3),
        normal: [0, 1, 0].map(|c| channel(NormalRoughness, c)),
        normal_encoding: SceneNormalEncoding::TwoChannelXy,
        roughness: channel(Height, 0),
        invert_roughness: false,
    }
}

fn mapped(terrain: SceneTerrain, map: TerrainLayerChannelMap) -> SceneTerrain {
    let mut source = terrain.layered_source().unwrap().clone();
    source.channel_map = Some(map);
    terrain.with_layered_source(source).unwrap()
}

fn texels(format: TexelFormat, space: TextureColorSpace, pixel: &[u8]) -> SceneTexture {
    SceneTexture::Texels {
        format,
        color_space: space,
        width: 4,
        height: 4,
        mips: 1,
        bytes: pixel.repeat(16).into_boxed_slice(),
    }
}

fn consumer_fixture() -> (SceneTerrain, SceneResources, SceneResources) {
    let (terrain, _) = super::source_mip_tests::fixture(2700, 1700, [1; 3]);
    let color = [
        173, 173, 0, 0, 0, 0, 0, 0, 0x10, 0x84, 0x10, 0x84, 0, 0, 0, 0,
    ];
    let normal = [153, 153, 0, 0, 0, 0, 0, 0, 102, 102, 0, 0, 0, 0, 0, 0];
    let scalars = [0x98, 0x41, 0x98, 0x41, 0, 0, 0, 0];
    let mut albedo = super::resident_tests::compressed(78, 4, 1, &color);
    if let SceneTexture::Encoded { color_space, .. } = &mut albedo {
        *color_space = TextureColorSpace::Srgb;
    }
    let compressed = SceneResources::new(
        vec![
            albedo,
            super::resident_tests::compressed(83, 4, 1, &normal),
            super::resident_tests::compressed(71, 4, 1, &scalars),
        ],
        vec![],
    )
    .unwrap();
    // Independent constant-block decoding: BC1 uses replicated RGB565 bits;
    // BC3 alpha and both BC5 components select endpoint zero everywhere.
    let decoded = SceneResources::new(
        vec![
            texels(
                TexelFormat::Rgba8Unorm,
                TextureColorSpace::Srgb,
                &[132, 130, 132, 173],
            ),
            texels(
                TexelFormat::Rg8Unorm,
                TextureColorSpace::Linear,
                &[153, 102],
            ),
            texels(
                TexelFormat::Rgba8Unorm,
                TextureColorSpace::Linear,
                &[66, 48, 198, 255],
            ),
        ],
        vec![],
    )
    .unwrap();
    (mapped(terrain, consumer_map()), compressed, decoded)
}

fn compare(terrain: &SceneTerrain, gpu: &SceneResources, cpu: &SceneResources) {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let source = LayeredGpuSource::new(terrain, gpu).unwrap();
    let reference = LayeredReference::new(terrain, cpu).unwrap();
    let positions = [[0.1, 0.3], [1.25, 0.75], [-0.25, -0.125]];
    let actual = super::tests::execute(&context, &source, &positions);
    for (position, actual) in positions.into_iter().zip(actual.chunks_exact(16)) {
        let (material, height) = reference.sample(position).unwrap().pack();
        let expected = material
            .color
            .into_iter()
            .chain(material.normal_roughness)
            .chain([height]);
        for (index, (actual, expected)) in actual.iter().copied().zip(expected).enumerate() {
            assert!(
                actual.abs_diff(expected) <= 1,
                "product component {index}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn bc3_srgb_height_bc5_xy_bc1_roughness_metallic_matches_every_cpu_product() {
    let (terrain, compressed, decoded) = consumer_fixture();
    compare(&terrain, &compressed, &decoded);
    let source = LayeredGpuSource::new(&terrain, &compressed).unwrap();
    assert_eq!(super::textures::payload_bytes(&source.arrays), 40);
    assert_eq!(source.arrays.len(), 3);
}

#[test]
fn custom_swizzles_xyz_and_smoothness_match_every_cpu_product() {
    let (terrain, _) = super::source_mip_tests::fixture(1700, 900, [1; 3]);
    let mut map =
        TerrainLayerChannelMap::packed(SceneNormalEncoding::Rgb, TextureColorSpace::Linear);
    map.albedo = [2, 0, 1].map(|c| channel(TerrainLayerTexture::Height, c));
    map.height = channel(TerrainLayerTexture::AlbedoMetallic, 2);
    map.metallic = channel(TerrainLayerTexture::NormalRoughness, 0);
    map.normal = [2, 1, 0].map(|c| channel(TerrainLayerTexture::NormalRoughness, c));
    map.roughness = channel(TerrainLayerTexture::AlbedoMetallic, 1);
    map.invert_roughness = true;
    let resources = SceneResources::new(
        vec![
            texels(
                TexelFormat::Rgba8Unorm,
                TextureColorSpace::Linear,
                &[40, 90, 220, 180],
            ),
            texels(
                TexelFormat::Rgba8Unorm,
                TextureColorSpace::Linear,
                &[230, 160, 100, 70],
            ),
            texels(
                TexelFormat::Rgba8Unorm,
                TextureColorSpace::Linear,
                &[70, 140, 210, 30],
            ),
        ],
        vec![],
    )
    .unwrap();
    compare(&mapped(terrain, map), &resources, &resources);
}

#[test]
fn explicit_default_map_is_byte_identical_to_implicit_packing() {
    let context = context();
    let (terrain, resources) = super::tests::fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    let implicit = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let layer = terrain.surfaces()[0].appearance().layered.unwrap();
    let map = terrain
        .layered_source()
        .unwrap()
        .channels(layer, resources.textures())
        .unwrap();
    let terrain = mapped(terrain, map);
    let explicit = LayeredGpuSource::new(&terrain, &resources).unwrap();
    assert_eq!(implicit.parameters, explicit.parameters);
    let positions = [[0.1, 0.3], [1.25, 0.75], [-0.25, -0.125]];
    assert_eq!(
        super::tests::execute(&context, &implicit, &positions),
        super::tests::execute(&context, &explicit, &positions)
    );
}

#[test]
fn channel_maps_reject_missing_components_handles_and_color_mismatches() {
    let (terrain, _, resources) = consumer_fixture();
    let layer = terrain.surfaces()[0].appearance().layered.unwrap();
    let valid = consumer_map();
    let mut bad = valid;
    bad.height.channel = 4;
    assert!(bad.validate(layer, resources.textures()).is_err());
    bad = valid;
    bad.normal[0].channel = 2;
    assert!(bad.validate(layer, resources.textures()).is_err());
    bad = valid;
    bad.albedo_color_space = TextureColorSpace::Linear;
    assert!(bad.validate(layer, resources.textures()).is_err());
    bad = valid;
    bad.roughness = channel(TerrainLayerTexture::AlbedoMetallic, 0);
    assert!(bad.validate(layer, resources.textures()).is_err());
    let missing = TerrainLayer {
        height: TextureHandle::from_raw(99),
        ..layer
    };
    assert!(matches!(
        valid.validate(missing, resources.textures()),
        Err(SceneValidationError::HandleOutOfRange { handle: 99, .. })
    ));
    let terrain = mapped(terrain, bad);
    assert!(LayeredGpuSource::new(&terrain, &resources).is_err());
    assert!(LayeredReference::new(&terrain, &resources).is_err());
}
