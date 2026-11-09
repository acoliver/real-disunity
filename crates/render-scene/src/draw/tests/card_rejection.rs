//! Base-shader attachment comparisons with rejected quad neighbours and cascaded casters.
#[path = "card_rejection_boundaries.rs"]
mod boundaries;
use super::*;
use worldrunner_render_plan::{
    SceneAtlasSampling, SceneDecal, SceneDecalAxis, SceneDecalColorModifier, SceneDecalMapping,
    SceneDecalReceivers, SceneDecalWriteMask, SceneFadeBand, SceneImpostorChannels,
};

fn atlas(kind: u32) -> SceneTexture {
    let mut bytes = Vec::new();
    for level in 0..6 {
        let size = (48_u32 >> level).max(1);
        for y in 0..size {
            for x in 0..size {
                let a = u8::try_from((x * 31 + y * 13 + level * 47) % 256).unwrap();
                let b = u8::try_from((x * 7 + y * 29 + level * 19) % 256).unwrap();
                let texel = match kind {
                    0 => [a, b, 180, if (x + y) % 5 == 0 { 0 } else { a }],
                    1 => [115 + a / 10, 115 + b / 10, 230, 255],
                    2 => [a / 2, b, 128, 255],
                    3 => [80 + a / 2, 30 + b / 2, 90, 255],
                    _ => [a / 5, 80 + b / 2, 60, 255],
                };
                bytes.extend_from_slice(&texel);
            }
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 48,
        height: 48,
        mips: 6,
        bytes: bytes.into(),
    }
}

fn materials(first: u32) -> Vec<SceneMaterial> {
    (0..4)
        .map(|index| {
            let mut atlas = SceneImpostor::new(
                SceneImpostorLayout::HemisphericalOctahedral,
                [3, 3],
                10_000,
                true,
            )
            .unwrap()
            .with_sampling(SceneAtlasSampling::Implicit)
            .with_pass_cutoffs(
                if index < 2 { 500_000 } else { 250_000 },
                if index < 2 { 470_000 } else { 230_000 },
            )
            .unwrap();
            if index % 2 == 0 {
                atlas = atlas
                    .with_relief(TextureHandle::from_raw(first + 2), 130)
                    .unwrap();
            }
            if index < 2 {
                atlas =
                    atlas.with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion);
            }
            SceneMaterial::new(SceneMaterialParts {
                albedo: TextureHandle::from_raw(first),
                normal: Some(TextureHandle::from_raw(first + 1)),
                roughness_metallic: Some(TextureHandle::from_raw(first + 3)),
                ambient_occlusion: Some(TextureHandle::from_raw(first + 4)),
                self_illumination: Some(TextureHandle::from_raw(first + 4)),
                roughness_micro: 650_000,
                metalness_micro: 170_000,
                alpha_mode: SceneAlphaMode::Mask {
                    cutoff_micro: 500_000,
                },
                double_sided: true,
            })
            .unwrap()
            .with_features(worldrunner_render_plan::SceneMaterialFeatures {
                tint: worldrunner_render_plan::SceneTint::WHITE,
                ambient_occlusion: worldrunner_render_plan::SceneAmbientOcclusion::UNOCCLUDED,
                wind: Some(
                    worldrunner_render_plan::SceneWind::new(
                        SceneDirection::new([1_000_000, 0, 0]).unwrap(),
                        100,
                        1000,
                        0,
                    )
                    .unwrap(),
                ),
                layer: None,
                translucency: None,
            })
            .unwrap()
            .with_impostor(atlas)
            .with_translucency(650_000)
            .unwrap()
        })
        .collect()
}

fn cards(material: u32, mesh: u32) -> Vec<SceneObject> {
    (0_u32..72)
        .flat_map(|i| {
            let object = SceneObject::new(
                SceneTransform {
                    position_millimeters: [
                        OFFSET[0] + i64::from(i % 9) * 320 - 1_300,
                        OFFSET[1] + i64::from(i / 9) * 500 - 1_500,
                        450,
                    ],
                    ..identity()
                },
                MeshHandle::from_raw(mesh),
                MaterialHandle::from_raw(material + i % 4),
            );
            // Two materials share the same plane, so the first card's holes must not
            // consume the second card's stencil ownership.
            let tie = SceneObject::new(
                *object.transform(),
                MeshHandle::from_raw(mesh),
                MaterialHandle::from_raw(material + (i + 2) % 4),
            );
            let band = match i % 6 {
                0 => SceneFadeBand::new(0, 1).unwrap(),
                1 => SceneFadeBand::new(100_000, 100_000).unwrap(),
                2 => SceneFadeBand::new(7_000, 12_000).unwrap(),
                3 => SceneFadeBand::new(7_000, 12_000)
                    .unwrap()
                    .with_terminal_fade(),
                4 => SceneFadeBand::new(8_000, 8_000).unwrap(),
                _ => SceneFadeBand::new(8_000, 8_000)
                    .unwrap()
                    .with_terminal_fade(),
            };
            [object.with_fade(band), tie]
        })
        .collect()
}

pub(super) fn fixture() -> RenderScene {
    let base = sloped(false);
    let mut textures = base.resources().textures().to_vec();
    let decal_normal = u32::try_from(textures.len()).unwrap();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 1,
        bytes: vec![127, 127, 140, 116, 110, 145, 135, 120].into(),
    });
    let first = u32::try_from(textures.len()).unwrap();
    textures.extend((0..5).map(atlas));
    let mut resources = base.resources().materials().to_vec();
    let material = u32::try_from(resources.len()).unwrap();
    resources.extend(materials(first));
    let decal_material = u32::try_from(resources.len()).unwrap();
    resources.push(
        SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(first),
            normal: Some(TextureHandle::from_raw(decal_normal)),
            roughness_metallic: Some(TextureHandle::from_raw(first + 3)),
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 650_000,
            metalness_micro: 170_000,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: true,
        })
        .unwrap(),
    );
    let mut meshes = base.geometry().meshes().to_vec();
    let mesh = u32::try_from(meshes.len()).unwrap();
    meshes.push(impostor::relief_scene().geometry().meshes()[0].clone());
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, resources).unwrap(),
        geometry: SceneGeometry::new(meshes, cards(material, mesh)).unwrap(),
        terrain: base.terrain().cloned(),
        effects: base.effects().cloned(),
    })
    .unwrap();
    source
        .with_decals(vec![SceneDecal {
            priority: 0,
            transform: SceneTransform {
                position_millimeters: [OFFSET[0], OFFSET[1], 500],
                ..identity()
            },
            dimensions_millimeters: [6_000, 6_000, 4_000],
            material: MaterialHandle::from_raw(decal_material),
            axis: SceneDecalAxis::Y,
            edge_fade_micro: 100_000,
            angle_cutoff_micro: 0,
            angle_fade_micro: 200_000,
            receivers: SceneDecalReceivers::MESH,
            mapping: SceneDecalMapping {
                double_sided: true,
                ..Default::default()
            },
            color_modifier: SceneDecalColorModifier::default(),
            write_mask: SceneDecalWriteMask::ALL,
            alpha_test_cutoff_micro: Some(100_000),
        }])
        .unwrap()
}

#[test]
fn card_rejection_fixture_uses_supported_decals() {
    crate::draw::decals::validate_decal_materials(&fixture()).unwrap();
}

#[test]
fn gpu_paired_card_rejection_matches_base_all_attachment_bytes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = fixture();
    let mut total = 0;
    for bias in [0.0, 0.012] {
        for distance in [8_000, 16_000, 31_997] {
            let mut pose = receiver_pose();
            pose.eye_millimeters[1] = OFFSET[1] - distance;
            let old = frame_at_pose_motion(
                &context,
                &source,
                Some(ReceiverReference::LateCardRejection),
                bias,
                pose,
                true,
            );
            let new = frame_at_pose_motion(&context, &source, None, bias, pose, true);
            assert_eq!(old.len(), new.len());
            assert!(
                new[5].iter().any(|byte| *byte != 0),
                "wind must write nonzero velocity"
            );
            let casters = &new[new.len() - 3];
            assert!(
                casters.chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()),
                "cascades must contain casters"
            );
            assert!(
                new[new.len() - 2].iter().any(|byte| *byte != 0),
                "receiver payload must be populated"
            );
            for (attachment, (a, b)) in old.iter().zip(&new).enumerate() {
                let differences: Vec<_> = a
                    .iter()
                    .zip(b)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .take(8)
                    .collect();
                if !differences.is_empty() {
                    println!(
                        "CARD_REJECTION_DIFF distance={distance} bias={bias} attachment={attachment} first={differences:?}"
                    );
                }
            }
            for (attachment, (old, new)) in old.iter().zip(new).enumerate() {
                assert_eq!(old.len(), new.len());
                assert_eq!(
                    old.iter().zip(&new).filter(|(a, b)| a != b).count(),
                    0,
                    "distance={distance} bias={bias} attachment={attachment}"
                );
                total += old.len();
            }
        }
    }
    println!("CARD_REJECTION_BASE_BYTES {total}");
}
