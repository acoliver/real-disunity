use super::*;
use crate::{SceneImpostor, SceneImpostorChannels, SceneImpostorLayout};
use std::sync::Arc;

fn material_parts() -> SceneMaterialParts {
    SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(1)),
        ambient_occlusion: Some(TextureHandle::from_raw(2)),
        self_illumination: Some(TextureHandle::from_raw(2)),
        roughness_micro: 700_000,
        metalness_micro: 200_000,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 400_000,
        },
        double_sided: true,
    }
}

fn impostor() -> SceneImpostor {
    SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [2, 2], 0, true)
        .unwrap()
        .with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion)
        .with_relief(TextureHandle::from_raw(1), 500)
        .unwrap()
        .with_borders(TextureHandle::from_raw(2))
}

fn textures() -> Vec<SceneTexture> {
    let mut textures = Vec::with_capacity(8);
    textures.push(SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Srgb,
        bytes: vec![1, 2, 3].into_boxed_slice(),
    });
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 2,
        bytes: vec![7; 10].into_boxed_slice(),
    });
    textures.push(SceneTexture::Encoded {
        encoding: TextureEncoding::KtxContainer,
        color_space: TextureColorSpace::Linear,
        bytes: vec![4, 5].into_boxed_slice(),
    });
    textures
}

fn resources() -> SceneResources {
    let material = SceneMaterial::new(material_parts())
        .unwrap()
        .with_impostor(impostor());
    SceneResources::new(textures(), vec![material]).unwrap()
}

fn payload(texture: &SceneTexture) -> *const u8 {
    match texture {
        SceneTexture::Sampled { texture, .. } => payload(texture),
        SceneTexture::Texels { bytes, .. } | SceneTexture::Encoded { bytes, .. } => bytes.as_ptr(),
    }
}

#[test]
fn replacing_materials_shares_textures_and_preserves_the_clone() {
    let original = resources();
    let clone = original.clone();
    let texture_table = original.textures().as_ptr();
    let old_materials = original.materials().as_ptr();
    let payloads: Vec<_> = original.textures().iter().map(payload).collect();
    let replacement = SceneMaterial::new(material_parts()).unwrap();
    let materials = vec![replacement, replacement];
    let new_materials = materials.as_ptr();
    let replaced = original.with_materials(materials).unwrap();
    assert_eq!(replaced.textures().as_ptr(), texture_table);
    assert_eq!(clone.textures().as_ptr(), texture_table);
    assert_eq!(clone.materials().as_ptr(), old_materials);
    assert_eq!(clone.materials().len(), 1);
    assert_eq!(clone.materials()[0].impostor(), Some(impostor()));
    assert_eq!(replaced.materials().as_ptr(), new_materials);
    assert_eq!(replaced.materials(), &[replacement, replacement]);
    assert_eq!(
        replaced.textures().iter().map(payload).collect::<Vec<_>>(),
        payloads
    );
}

#[test]
fn owned_tables_preserve_storage_payloads_and_values_through_append() {
    let textures = textures();
    let texture_table = textures.as_ptr();
    let texture_capacity = textures.capacity();
    let expected = textures.clone();
    let material = SceneMaterial::new(material_parts())
        .unwrap()
        .with_impostor(impostor());
    let mut materials = Vec::with_capacity(4);
    materials.push(material);
    let material_table = materials.as_ptr();
    let material_capacity = materials.capacity();
    let payloads: Vec<_> = textures.iter().map(payload).collect();
    let resources = SceneResources::new(textures, materials).unwrap();
    assert_eq!(resources.textures().as_ptr(), texture_table);
    assert_eq!(resources.materials().as_ptr(), material_table);
    let (mut textures, mut materials) = resources.try_into_tables().unwrap();
    assert_eq!(textures.as_ptr(), texture_table);
    assert_eq!(materials.as_ptr(), material_table);
    assert_eq!(textures.capacity(), texture_capacity);
    assert_eq!(materials.capacity(), material_capacity);
    textures.push(SceneTexture::Encoded {
        encoding: TextureEncoding::OpaqueExact,
        color_space: TextureColorSpace::Linear,
        bytes: Box::default(),
    });
    materials.push(material);
    let resources = SceneResources::new(textures, materials).unwrap();
    assert_eq!(resources.textures().as_ptr(), texture_table);
    assert_eq!(resources.materials().as_ptr(), material_table);
    assert_eq!(&resources.textures()[..3], &expected);
    assert_eq!(
        resources.textures()[..3]
            .iter()
            .map(payload)
            .collect::<Vec<_>>(),
        payloads
    );
    assert_eq!(resources.materials(), &[material, material]);
    assert_eq!(resources.materials()[0].impostor(), Some(impostor()));
    assert_eq!(resources.validate(), Ok(()));
}

#[test]
fn either_shared_table_blocks_extraction_without_changing_either_table() {
    for (share_textures, share_materials) in [(true, false), (false, true), (true, true)] {
        let original = resources();
        let texture_table = original.textures().as_ptr();
        let material_table = original.materials().as_ptr();
        let texture_owner = Arc::as_ptr(&original.textures);
        let material_owner = Arc::as_ptr(&original.materials);
        let payloads: Vec<_> = original.textures().iter().map(payload).collect();
        let textures = share_textures.then(|| Arc::clone(&original.textures));
        let materials = share_materials.then(|| Arc::clone(&original.materials));
        let returned = original.try_into_tables().unwrap_err();
        assert_eq!(returned.textures().as_ptr(), texture_table);
        assert_eq!(returned.materials().as_ptr(), material_table);
        assert_eq!(Arc::as_ptr(&returned.textures), texture_owner);
        assert_eq!(Arc::as_ptr(&returned.materials), material_owner);
        assert_eq!(
            returned.textures().iter().map(payload).collect::<Vec<_>>(),
            payloads
        );
        assert_eq!(
            Arc::strong_count(&returned.textures),
            1 + usize::from(share_textures)
        );
        assert_eq!(
            Arc::strong_count(&returned.materials),
            1 + usize::from(share_materials)
        );
        assert_eq!(returned.materials()[0].impostor(), Some(impostor()));
        drop((textures, materials));
        let (textures, materials) = returned.try_into_tables().unwrap();
        assert_eq!(textures.as_ptr(), texture_table);
        assert_eq!(materials.as_ptr(), material_table);
        assert_eq!(textures.iter().map(payload).collect::<Vec<_>>(), payloads);
    }
}

#[test]
fn a_live_resources_clone_blocks_extraction_until_dropped() {
    let original = resources();
    let clone = original.clone();
    assert_eq!(original.materials().as_ptr(), clone.materials().as_ptr());
    let original = original.try_into_tables().unwrap_err();
    assert_eq!(original, clone);
    assert_eq!(original.textures().as_ptr(), clone.textures().as_ptr());
    assert_eq!(original.materials().as_ptr(), clone.materials().as_ptr());
    drop(clone);
    assert!(original.try_into_tables().is_ok());
}

#[test]
fn replacement_rejects_every_out_of_range_material_texture_channel() {
    let bad = TextureHandle::from_raw(3);
    for (part, parts) in [
        (
            "material albedo",
            SceneMaterialParts {
                albedo: bad,
                ..material_parts()
            },
        ),
        (
            "material normal",
            SceneMaterialParts {
                normal: Some(bad),
                ..material_parts()
            },
        ),
        (
            "material roughness_metallic",
            SceneMaterialParts {
                roughness_metallic: Some(bad),
                ..material_parts()
            },
        ),
        (
            "material ambient_occlusion",
            SceneMaterialParts {
                ambient_occlusion: Some(bad),
                ..material_parts()
            },
        ),
        (
            "material self_illumination",
            SceneMaterialParts {
                self_illumination: Some(bad),
                ..material_parts()
            },
        ),
    ] {
        let material = SceneMaterial::new(parts).unwrap();
        let expected = SceneValidationError::HandleOutOfRange {
            part,
            resource: "texture",
            handle: 3,
            limit: 3,
        };
        assert_eq!(
            resources().with_materials(vec![material]),
            Err(expected.clone())
        );
        assert_eq!(
            SceneResources::new(textures(), vec![material]),
            Err(expected)
        );
    }
}

#[test]
fn replacement_rejects_impostor_depth_borders_and_missing_packed_channels() {
    let material = SceneMaterial::new(material_parts()).unwrap();
    for (atlas, part) in [
        (
            impostor()
                .with_relief(TextureHandle::from_raw(3), 1)
                .unwrap(),
            "impostor depth",
        ),
        (
            impostor().with_borders(TextureHandle::from_raw(3)),
            "impostor borders",
        ),
    ] {
        let replacement = material.with_impostor(atlas);
        let expected = SceneValidationError::HandleOutOfRange {
            part,
            resource: "texture",
            handle: 3,
            limit: 3,
        };
        assert_eq!(
            resources().with_materials(vec![replacement]),
            Err(expected.clone())
        );
        assert_eq!(
            SceneResources::new(textures(), vec![replacement]),
            Err(expected)
        );
    }
    let missing = SceneMaterial::new(SceneMaterialParts {
        roughness_metallic: None,
        ..material_parts()
    })
    .unwrap()
    .with_impostor(impostor());
    let expected = SceneValidationError::UnsupportedInput {
        part: "packed impostor requires RT and emission-occlusion atlases",
    };
    assert_eq!(
        resources().with_materials(vec![missing]),
        Err(expected.clone())
    );
    assert_eq!(
        SceneResources::new(textures(), vec![missing]),
        Err(expected)
    );
}

#[test]
fn replacement_revalidates_texture_payloads_before_material_handles() {
    let textures = vec![SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Srgb,
        bytes: Box::default(),
    }];
    let expected = SceneResources::new(textures.clone(), vec![]).unwrap_err();
    let invalid = SceneResources {
        textures: textures.into(),
        materials: vec![].into(),
    };
    assert_eq!(
        invalid.with_materials(vec![SceneMaterial::new(material_parts()).unwrap()]),
        Err(expected)
    );
}

#[test]
fn empty_tables_can_be_shared_replaced_and_extracted() {
    let empty = SceneResources::new(vec![], vec![]).unwrap();
    let clone = empty.clone();
    let empty = empty.with_materials(vec![]).unwrap();
    let empty = empty.try_into_tables().unwrap_err();
    assert_eq!(empty, clone);
    drop(clone);
    assert_eq!(empty.try_into_tables().unwrap(), (vec![], vec![]));
}

#[test]
fn replacement_validates_layers_against_the_new_material_table() {
    use crate::{
        SceneAmbientOcclusion, SceneLayerBlend, SceneLayerMode, SceneMaterialFeatures, SceneTint,
    };
    let base = SceneMaterial::new(material_parts()).unwrap();
    let layered = base
        .with_features(SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: None,
            translucency: None,
            layer: Some(
                SceneLayerBlend::new(MaterialHandle::from_raw(1), 500_000, SceneLayerMode::Linear)
                    .unwrap(),
            ),
        })
        .unwrap();
    let original = resources().with_materials(vec![layered, base]).unwrap();
    let expected = SceneValidationError::HandleOutOfRange {
        part: "material layer",
        resource: "material",
        handle: 1,
        limit: 1,
    };
    assert_eq!(
        original.with_materials(vec![layered]),
        Err(expected.clone())
    );
    assert_eq!(
        SceneResources::new(textures(), vec![layered]),
        Err(expected)
    );
    let expected = SceneValidationError::UnsupportedInput {
        part: "nested material layers",
    };
    assert_eq!(
        resources().with_materials(vec![layered, layered]),
        Err(expected.clone())
    );
    assert_eq!(
        SceneResources::new(textures(), vec![layered, layered]),
        Err(expected)
    );
}
