//! Ordinary surface coverage alongside cards and terrain.
use super::*;

pub(super) fn fixture() -> RenderScene {
    let base = card_rejection::fixture();
    let mut textures = base.resources().textures().to_vec();
    let albedo = u32::try_from(textures.len()).unwrap();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 4,
        mips: 1,
        bytes: (0..16)
            .flat_map(|i| [180, 95, 45, if i % 3 == 0 { 0 } else { 255 }])
            .collect::<Vec<_>>()
            .into(),
    });
    let mut materials = base.resources().materials().to_vec();
    let first = u32::try_from(materials.len()).unwrap();
    for index in 0..3 {
        materials.push(
            SceneMaterial::new(SceneMaterialParts {
                albedo: TextureHandle::from_raw(albedo),
                normal: None,
                roughness_metallic: None,
                ambient_occlusion: None,
                self_illumination: None,
                roughness_micro: 650_000,
                metalness_micro: 0,
                alpha_mode: if index == 0 {
                    SceneAlphaMode::Opaque
                } else {
                    SceneAlphaMode::Mask {
                        cutoff_micro: 500_000,
                    }
                },
                double_sided: true,
            })
            .unwrap()
            .with_translucency(if index == 2 { 850_000 } else { 0 })
            .unwrap(),
        );
    }
    let mut meshes = base.geometry().meshes().to_vec();
    let mesh = u32::try_from(meshes.len()).unwrap();
    meshes.push(card(None));
    let mut objects = base.geometry().objects().to_vec();
    for index in 0..3_u32 {
        objects.push(SceneObject::new(
            SceneTransform {
                position_millimeters: [
                    OFFSET[0] + i64::from(index) * 1_600 - 1_600,
                    OFFSET[1] - 3_500,
                    800,
                ],
                scale_millionths: 650_000,
                ..identity()
            },
            MeshHandle::from_raw(mesh),
            MaterialHandle::from_raw(first + index),
        ));
    }
    let mut parts = effect_parts();
    parts.sun.direction = SceneDirection::new([0, 800_000, 600_000]).unwrap();
    parts.anti_alias = SceneAntiAlias::taa(900_000);
    parts.ssao = Some(crate::ssr::neutral_ao());
    parts.shadows =
        SceneShadowCascades::camera_splits(vec![12_000, 40_000, 150_000], 300_000).unwrap();
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_ssr(*base.effects().unwrap().ssr().unwrap())
        .unwrap();
    base.with_resources(
        SceneResources::new(textures, materials).unwrap(),
        SceneGeometry::new(meshes, objects).unwrap(),
        Some(effects),
    )
    .unwrap()
}

pub(super) fn without_surface(source: &RenderScene, index: usize) -> RenderScene {
    let mut objects = source.geometry().objects().to_vec();
    let first = objects.len() - 3;
    objects.remove(first + index);
    source
        .with_objects_and_effects(objects, source.effects().cloned())
        .unwrap()
}

pub(super) fn without_transmission(source: &RenderScene) -> RenderScene {
    let mut materials = source.resources().materials().to_vec();
    let last = materials.len() - 1;
    materials[last] = materials[last].with_translucency(0).unwrap();
    source
        .with_resources(
            SceneResources::new(source.resources().textures().to_vec(), materials).unwrap(),
            source.geometry().clone(),
            source.effects().cloned(),
        )
        .unwrap()
}

#[test]
fn poison_fixture_contains_ordinary_opaque_masked_and_transmitting_surfaces() {
    let scene = fixture();
    let materials = scene.resources().materials();
    let ordinary = &materials[materials.len() - 3..];
    assert!(
        ordinary
            .iter()
            .all(|material| material.impostor().is_none())
    );
    assert_eq!(ordinary[0].alpha_mode(), SceneAlphaMode::Opaque);
    for material in &ordinary[1..] {
        assert_eq!(
            material.alpha_mode(),
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000
            }
        );
    }
    assert_eq!(ordinary[0].translucency_micro(), 0);
    assert_eq!(ordinary[1].translucency_micro(), 0);
    assert!(ordinary[2].translucency_micro() > 0);
}
