//! Behavioural claims about releasing CPU texture sources into validated descriptors.

use super::super::{TextureError, prepare_texture};
use super::{DXGI_BC1_UNORM, DXGI_BC1_UNORM_SRGB, DXGI_BC7_UNORM, dds, encoded_sized};
use crate::draw::{DrawError, PreparedScene};
use crate::texture::{TextureReleaseError, describe_texture, release_texture_sources};
use worldrunner_render_plan::{
    RenderScene, SceneAlphaMode, SceneCubeDescriptor, SceneDdsDescriptor, SceneDdsFormat,
    SceneGeometry, SceneMaterial, SceneMaterialParts, SceneParts, SceneResources,
    SceneSpecularConvention, SceneSpecularCube, SceneSpecularCubeFormat, SceneSpecularPreparation,
    SceneTexelDescriptor, SceneTexture, SceneTextureDescriptor, TexelFormat, TextureColorSpace,
    TextureEncoding, TextureFilter, TextureHandle, TextureSampler, TextureWrap,
};

const TEXEL_BYTES: u64 = 20;
const DDS_BYTES: u64 = 148 + 112;
const CUBE_BYTES: u64 = 288;
const PREPARED_CUBE_BYTES: u64 = 768;
const TOTAL_BYTES: u64 = TEXEL_BYTES + DDS_BYTES + CUBE_BYTES + PREPARED_CUBE_BYTES;

fn sampler() -> TextureSampler {
    TextureSampler {
        filter: TextureFilter::Point,
        wrap_u: TextureWrap::Clamp,
        wrap_v: TextureWrap::Mirror,
    }
}

fn preparation() -> SceneSpecularPreparation {
    SceneSpecularPreparation {
        convention: SceneSpecularConvention::default(),
        cube_size: 4,
        mip_count: 3,
        samples: 8,
        source_lod_tail: 1,
    }
}

fn material() -> SceneMaterial {
    material_with_roughness_metallic(Some(TextureHandle::from_raw(1)))
}

fn material_with_roughness_metallic(roughness_metallic: Option<TextureHandle>) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 250_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

fn scene(resources: SceneResources) -> RenderScene {
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn texels() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 2,
        bytes: vec![0x31; 20].into(),
    }
    .with_sampler(sampler())
}

fn bc7_chain() -> SceneTexture {
    encoded_sized(DXGI_BC7_UNORM, TextureColorSpace::Linear, 8, 8, 4, 112)
}

fn sources() -> SceneResources {
    let cube = SceneSpecularCube::new(4, vec![0x6a; 288].into_boxed_slice()).unwrap();
    let prepared = SceneSpecularCube::rgba16_float(4, 1, vec![0x2b; 768].into_boxed_slice())
        .unwrap()
        .with_preparation(preparation())
        .unwrap();
    SceneResources::new(
        vec![
            texels(),
            bc7_chain(),
            SceneTexture::SpecularCube(cube),
            SceneTexture::SpecularCube(prepared),
        ],
        vec![material()],
    )
    .unwrap()
}

fn expected_descriptors() -> [SceneTextureDescriptor; 4] {
    [
        SceneTextureDescriptor::Texels(SceneTexelDescriptor {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: 2,
            height: 2,
            mips: 2,
            logical_bytes: TEXEL_BYTES,
        }),
        SceneTextureDescriptor::Dds(SceneDdsDescriptor {
            format: SceneDdsFormat::Bc7Rgba,
            color_space: TextureColorSpace::Linear,
            width: 8,
            height: 8,
            mips: 4,
            logical_bytes: DDS_BYTES,
        }),
        SceneTextureDescriptor::SpecularCube(SceneCubeDescriptor {
            format: SceneSpecularCubeFormat::Bc6hRgbUfloat,
            size: 4,
            mips: 3,
            logical_bytes: CUBE_BYTES,
            preparation: None,
        }),
        SceneTextureDescriptor::SpecularCube(SceneCubeDescriptor {
            format: SceneSpecularCubeFormat::Rgba16Float,
            size: 4,
            mips: 1,
            logical_bytes: PREPARED_CUBE_BYTES,
            preparation: Some(preparation()),
        }),
    ]
}

fn descriptors(resources: &SceneResources) -> Vec<SceneTextureDescriptor> {
    resources
        .textures()
        .iter()
        .map(|texture| describe_texture(texture).expect("every fixture texture is valid"))
        .collect()
}

#[test]
fn release_keeps_validated_metadata_and_drops_source_backing() {
    let source = sources();
    let caller_clone = source.clone();
    assert_eq!(descriptors(&source), expected_descriptors());
    assert_eq!(source.logical_texture_bytes(), TOTAL_BYTES);
    assert_eq!(source.retained_texture_source_bytes(), TOTAL_BYTES);

    let released = release_texture_sources(&source).expect("all four fixtures are valid");

    assert_eq!(descriptors(&released), expected_descriptors());
    assert_eq!(released.logical_texture_bytes(), TOTAL_BYTES);
    assert_eq!(released.retained_texture_source_bytes(), 0);
    assert_eq!(released.textures().len(), source.textures().len());
    assert_eq!(released.materials(), source.materials());
    assert_eq!(released.materials()[0].albedo().raw(), 0);
    assert_eq!(
        released.materials()[0].roughness_metallic().unwrap().raw(),
        1
    );
    assert_eq!(released.textures()[0].sampler(), sampler());
    assert_eq!(
        released.textures()[1].sampler(),
        source.textures()[1].sampler()
    );
    assert!(released.validate().is_ok());

    assert_eq!(caller_clone, source);
    assert_eq!(caller_clone.retained_texture_source_bytes(), TOTAL_BYTES);
    assert!(
        caller_clone
            .textures()
            .iter()
            .all(|texture| prepare_texture(texture).is_ok())
    );
    assert!(PreparedScene::new(&scene(caller_clone)).is_ok());
}

#[test]
fn release_is_idempotent() {
    let released = release_texture_sources(&sources()).unwrap();
    let again = release_texture_sources(&released).expect("released tables convert again");
    assert_eq!(again, released);
    assert_eq!(again.retained_texture_source_bytes(), 0);
    assert_eq!(again.logical_texture_bytes(), TOTAL_BYTES);
}

#[test]
fn dds_descriptor_records_every_validated_header_field() {
    let srgb = encoded_sized(DXGI_BC1_UNORM_SRGB, TextureColorSpace::Srgb, 8, 4, 2, 24);
    let trailing = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: dds(DXGI_BC7_UNORM, 8, 8, 4, 120).into(),
    };
    let expected = |format, color_space, width, height, mips, logical_bytes| {
        SceneTextureDescriptor::Dds(SceneDdsDescriptor {
            format,
            color_space,
            width,
            height,
            mips,
            logical_bytes,
        })
    };
    assert_eq!(
        describe_texture(&srgb).unwrap(),
        expected(
            SceneDdsFormat::Bc1Rgba,
            TextureColorSpace::Srgb,
            8,
            4,
            2,
            148 + 24
        )
    );
    assert_eq!(
        describe_texture(&trailing).unwrap(),
        expected(
            SceneDdsFormat::Bc7Rgba,
            TextureColorSpace::Linear,
            8,
            8,
            4,
            148 + 120
        ),
        "logical bytes are the source length, including header and trailing bytes"
    );
}

#[test]
fn cube_descriptor_records_layout_and_preparation() {
    let [_, _, plain, prepared] = expected_descriptors();
    let (
        SceneTextureDescriptor::SpecularCube(plain),
        SceneTextureDescriptor::SpecularCube(prepared),
    ) = (plain, prepared)
    else {
        panic!("fixtures are cubes");
    };
    assert_eq!(plain.preparation, None);
    assert_eq!(prepared.preparation, Some(preparation()));
    assert_eq!(prepared.format, SceneSpecularCubeFormat::Rgba16Float);
    assert_eq!((plain.size, plain.mips), (4, 3));
}

#[test]
fn released_and_mixed_tables_validate_but_cannot_be_prepared() {
    let source = sources();
    let released = release_texture_sources(&source).unwrap();
    for texture in released.textures() {
        assert_eq!(
            prepare_texture(texture).unwrap_err(),
            TextureError::SourceReleased
        );
    }
    assert!(matches!(
        PreparedScene::new(&scene(released.clone())),
        Err(DrawError::Texture {
            index: 0,
            source: TextureError::SourceReleased
        })
    ));

    let mixed = SceneResources::new(
        vec![source.textures()[0].clone(), released.textures()[1].clone()],
        source.materials().to_vec(),
    )
    .expect("a mixed table keeps dense handles and material channels valid");
    assert!(prepare_texture(&mixed.textures()[0]).is_ok());
    assert_eq!(
        prepare_texture(&mixed.textures()[1]).unwrap_err(),
        TextureError::SourceReleased
    );
    assert!(matches!(
        PreparedScene::new(&scene(mixed)),
        Err(DrawError::Texture {
            index: 1,
            source: TextureError::SourceReleased
        })
    ));
}

#[test]
fn a_malformed_entry_aborts_release_and_leaves_the_table_unchanged() {
    let mut malformed = dds(DXGI_BC1_UNORM, 8, 8, 1, 32);
    malformed.truncate(147);
    let bad = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: malformed.into(),
    };
    let source = SceneResources::new(
        vec![texels(), bad],
        vec![material_with_roughness_metallic(None)],
    )
    .unwrap();
    let before = source.clone();

    let error = release_texture_sources(&source).unwrap_err();

    assert_eq!(
        error,
        TextureReleaseError::Texture {
            index: 1,
            source: TextureError::TruncatedHeader {
                needed: 148,
                available: 147
            }
        }
    );
    assert_eq!(source, before);
    assert_eq!(source.logical_texture_bytes(), TEXEL_BYTES + 147);
    assert_eq!(source.retained_texture_source_bytes(), TEXEL_BYTES + 147);
    assert_eq!(source.materials(), before.materials());
    assert_eq!(source.textures()[0].sampler(), sampler());
}

#[test]
fn release_rejects_every_source_that_preparation_rejects() {
    let r8_srgb = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0].into(),
    };
    let disagreeing = encoded_sized(DXGI_BC1_UNORM, TextureColorSpace::Srgb, 8, 8, 1, 32);
    let unsupported = encoded_sized(61, TextureColorSpace::Linear, 8, 8, 1, 32);
    let short_chain = encoded_sized(DXGI_BC7_UNORM, TextureColorSpace::Linear, 8, 8, 4, 111);
    let not_dds = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: vec![1, 2, 3].into(),
    };
    let ktx = SceneTexture::Encoded {
        encoding: TextureEncoding::KtxContainer,
        color_space: TextureColorSpace::Linear,
        bytes: vec![1, 2, 3].into(),
    };
    let opaque = SceneTexture::Encoded {
        encoding: TextureEncoding::OpaqueExact,
        color_space: TextureColorSpace::Linear,
        bytes: vec![].into(),
    };
    for texture in [
        r8_srgb,
        disagreeing,
        unsupported,
        short_chain,
        not_dds,
        ktx,
        opaque,
    ] {
        let expected = prepare_texture(&texture).unwrap_err();
        let table = SceneResources::new(vec![texels(), texture], vec![]).unwrap();
        assert_eq!(
            release_texture_sources(&table).unwrap_err(),
            TextureReleaseError::Texture {
                index: 1,
                source: expected
            }
        );
    }
}

/// Offsets of the `DDS_HEADER_DXT10` fields that describe the resource shape.
const OFFSET_RESOURCE_DIMENSION: usize = 132;
const OFFSET_MISC_FLAGS: usize = 136;
const OFFSET_ARRAY_SIZE: usize = 140;
const RESOURCE_DIMENSION_2D: u32 = 3;
const MISC_TEXTURE_CUBE: u32 = 4;
const CANONICAL_CHAIN_BYTES: usize = 112;

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// An 8x8 BC7 container with four mips whose shape fields are written explicitly, so the
/// fixture stays a plain two-dimensional single-layer texture whatever the shared helper
/// leaves in those fields.
fn canonical_2d_container(payload: usize) -> Vec<u8> {
    let mut bytes = dds(DXGI_BC7_UNORM, 8, 8, 4, payload);
    put_u32(&mut bytes, OFFSET_RESOURCE_DIMENSION, RESOURCE_DIMENSION_2D);
    put_u32(&mut bytes, OFFSET_MISC_FLAGS, 0);
    put_u32(&mut bytes, OFFSET_ARRAY_SIZE, 1);
    bytes
}

fn container_table(bytes: Vec<u8>) -> SceneResources {
    let container = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bytes.into(),
    };
    SceneResources::new(vec![container], Vec::new())
        .expect("a table without materials adopts any encoded payload")
}

fn assert_release_refuses_index_zero(bytes: Vec<u8>, what: &str) {
    let source = container_table(bytes);
    let before = source.clone();

    let outcome = release_texture_sources(&source);

    assert!(
        matches!(outcome, Err(TextureReleaseError::Texture { index: 0, .. })),
        "{what} must not become admitted metadata: {outcome:?}"
    );
    assert_eq!(source, before, "{what}: the source table is unchanged");
    assert_eq!(
        source.retained_texture_source_bytes(),
        source.logical_texture_bytes(),
        "{what}: the source payload is still held"
    );
}

#[test]
fn release_admits_the_canonical_two_dimensional_single_layer_container() {
    let bytes = canonical_2d_container(CANONICAL_CHAIN_BYTES);
    let source = container_table(bytes);

    let released = release_texture_sources(&source).expect("the canonical container releases");

    assert_eq!(
        describe_texture(&released.textures()[0]).unwrap(),
        expected_descriptors()[1]
    );
    assert_eq!(released.logical_texture_bytes(), DDS_BYTES);
    assert_eq!(released.retained_texture_source_bytes(), 0);
}

#[test]
fn release_preserves_accounting_for_bytes_after_the_mip_chain() {
    let bytes = canonical_2d_container(CANONICAL_CHAIN_BYTES + 1);
    let source = container_table(bytes);
    let before = source.clone();
    let released = release_texture_sources(&source).unwrap();
    assert_eq!(released.logical_texture_bytes(), DDS_BYTES + 1);
    assert_eq!(released.retained_texture_source_bytes(), 0);
    assert_eq!(source, before);
    assert_eq!(
        describe_texture(&released.textures()[0]).unwrap(),
        describe_texture(&source.textures()[0]).unwrap()
    );
}

#[test]
fn release_rejects_array_containers() {
    let mut bytes = canonical_2d_container(CANONICAL_CHAIN_BYTES);
    put_u32(&mut bytes, OFFSET_ARRAY_SIZE, 2);
    assert_release_refuses_index_zero(bytes, "two array layers");
}

#[test]
fn release_rejects_cube_containers() {
    let mut bytes = canonical_2d_container(CANONICAL_CHAIN_BYTES);
    put_u32(&mut bytes, OFFSET_MISC_FLAGS, MISC_TEXTURE_CUBE);
    assert_release_refuses_index_zero(bytes, "a cube container");
}

#[test]
fn release_rejects_containers_that_are_not_two_dimensional() {
    for dimension in [0, 2, 4] {
        let mut bytes = canonical_2d_container(CANONICAL_CHAIN_BYTES);
        put_u32(&mut bytes, OFFSET_RESOURCE_DIMENSION, dimension);
        assert_release_refuses_index_zero(bytes, &format!("resource dimension {dimension}"));
    }
}

#[test]
fn a_truncated_canonical_container_keeps_the_truncated_header_error() {
    for available in [147, 140] {
        let mut bytes = canonical_2d_container(CANONICAL_CHAIN_BYTES);
        bytes.truncate(available);
        let source = container_table(bytes);
        let expected = TextureError::TruncatedHeader {
            needed: 148,
            available,
        };

        assert_eq!(
            describe_texture(&source.textures()[0]).unwrap_err(),
            expected
        );
        assert_eq!(
            release_texture_sources(&source).unwrap_err(),
            TextureReleaseError::Texture {
                index: 0,
                source: expected
            }
        );
    }
}
