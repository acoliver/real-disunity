use super::{capture, decal, source, texel};
use crate::draw::tests::{context, effects, identity};
use glam::{Quat, Vec3};
use worldrunner_render_plan::{
    MaterialHandle, RenderScene, SceneAlphaMode, SceneGeometry, SceneMaterial, SceneMaterialParts,
    SceneMesh, SceneMeshLod, SceneObject, SceneResources, SceneTexture, TexelFormat,
    TextureColorSpace, TextureHandle,
};

#[test]
fn decal_normal_controls_reject_only_when_a_normal_map_is_present() {
    use worldrunner_render_plan::{SceneNormalEncoding, SceneNormalY};
    let base = mapped_scene(false);
    for normal in [None, Some(2)] {
        let default = material(normal, Some(3), 900_000, 100_000);
        for (candidate, supported) in [
            (default, true),
            (
                default.with_normal_encoding(SceneNormalEncoding::Rgb, SceneNormalY::Up),
                normal.is_none(),
            ),
            (
                default.with_normal_encoding(SceneNormalEncoding::TwoChannelXy, SceneNormalY::Down),
                normal.is_none(),
            ),
            (default.with_normal_scale(0).unwrap(), normal.is_none()),
        ] {
            let mut materials = base.resources().materials().to_vec();
            materials[1] = candidate;
            let scene = base
                .with_resources(
                    SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
                    base.geometry().clone(),
                    Some(effects()),
                )
                .unwrap()
                .with_decals(vec![decal()])
                .unwrap();
            assert_eq!(
                crate::draw::validate_decal_materials(&scene).is_ok(),
                supported
            );
        }
    }
}

fn material(
    normal: Option<u32>,
    surface: Option<u32>,
    roughness: u32,
    metalness: u32,
) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: normal.map(TextureHandle::from_raw),
        roughness_metallic: surface.map(TextureHandle::from_raw),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: roughness,
        metalness_micro: metalness,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}
fn mapped_scene(slope: bool) -> RenderScene {
    let base = source(slope, false);
    let mut textures = base.resources().textures().to_vec();
    textures[1] = texel([204, 51, 26, 255]);
    textures[2] = SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128, 191].into(),
    };
    textures[3] = texel([51, 249, 204, 255]);
    let mut materials = base.resources().materials().to_vec();
    materials[1] = material(Some(2), Some(3), 900_000, 100_000);
    base.with_resources(
        SceneResources::new(textures, materials).unwrap(),
        base.geometry().clone(),
        Some(effects()),
    )
    .unwrap()
}
fn compressed(scene: &RenderScene) -> RenderScene {
    let mut textures = scene.resources().textures().to_vec();
    let mut block = vec![0; 16];
    block[0] = 128;
    block[8] = 191;
    textures[2] = dds(83, block);
    let color = (0x06_u16 << 11) | (0x0f << 5) | 0x19;
    let mut block = vec![0; 8];
    block[..2].copy_from_slice(&color.to_le_bytes());
    textures[3] = dds(71, block);
    scene
        .with_resources(
            SceneResources::new(textures, scene.resources().materials().to_vec()).unwrap(),
            scene.geometry().clone(),
            Some(effects()),
        )
        .unwrap()
}
fn dds(format: u32, block: Vec<u8>) -> SceneTexture {
    let mut bytes = vec![0; 148];
    bytes[..4].copy_from_slice(b"DDS ");
    for (offset, value) in [
        (4, 124_u32),
        (12, 4),
        (16, 4),
        (28, 1),
        (76, 32),
        (128, format),
        (132, 3),
        (140, 1),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[84..88].copy_from_slice(b"DX10");
    bytes.extend(block);
    SceneTexture::Encoded {
        encoding: worldrunner_render_plan::TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bytes.into(),
    }
}
fn baked_scene(scene: &RenderScene, slope: bool, alpha: u16) -> RenderScene {
    let geometric = if slope {
        Vec3::new(-0.5, 0.0, 1.0).normalize()
    } else {
        Vec3::Z
    };
    let x = 1.0 / 255.0;
    let y = 127.0 / 255.0;
    let decoded = Vec3::new(y, -x, (1.0_f32 - x * x - y * y).sqrt());
    let corrected = Quat::from_rotation_arc(Vec3::Z, geometric) * decoded;
    let normal = (geometric
        .lerp(corrected, f32::from(alpha) / 255.0)
        .normalize()
        * 1_000_000.0)
        .round()
        .as_ivec3()
        .to_array();
    let mut vertices = scene.geometry().meshes()[0].lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.normal_micro = normal.map(i64::from);
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let mut materials = scene.resources().materials().to_vec();
    let (roughness, metalness) = match scene.resources().textures()[3] {
        SceneTexture::Encoded { .. } => (193_548, 806_452),
        _ => (200_000, 800_000),
    };
    let alpha = u32::from(alpha);
    materials[1] = material(
        None,
        None,
        (1_000_000 * (255 - alpha) + roughness * alpha) / 255,
        metalness * alpha / 255,
    );
    scene
        .with_resources(
            SceneResources::new(scene.resources().textures().to_vec(), materials).unwrap(),
            SceneGeometry::new(
                vec![mesh],
                vec![SceneObject::new(
                    identity(),
                    worldrunner_render_plan::MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(1),
                )],
            )
            .unwrap(),
            Some(effects()),
        )
        .unwrap()
}

#[test]
fn normal_and_packed_maps_match_independently_baked_plane_and_slope_before_lighting() {
    let context = context();
    for (slope, compressed_maps) in [(false, false), (true, false), (false, true), (true, true)] {
        let base = mapped_scene(slope);
        let base = if compressed_maps {
            compressed(&base)
        } else {
            base
        };
        let expected = baked_scene(&base, slope, 255);
        let mut d = decal();
        d.dimensions_millimeters = [4000; 3];
        d.edge_fade_micro = 0;
        d.angle_cutoff_micro = 0;
        d.angle_fade_micro = 0;
        let projected = base.with_decals(vec![d]).unwrap();
        for samples in crate::msaa_tests::supported_samples(&context) {
            let actual = capture(&context, &projected, samples);
            let expected = capture(&context, &expected, samples);
            for (x, y) in [(32, 32), (28, 32), (36, 32)] {
                let offset = (y * 64 + x) * 4;
                for (actual, expected) in actual[offset..offset + 3]
                    .iter()
                    .zip(&expected[offset..offset + 3])
                {
                    assert!(
                        actual.abs_diff(*expected) <= 1,
                        "slope={slope} samples={samples}: {actual} != {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn each_optional_channel_rejects_incompatible_shapes_and_color_formats() {
    let scene = mapped_scene(false);
    for channel in [2, 3] {
        let mut textures = scene.resources().textures().to_vec();
        let mut materials = scene.resources().materials().to_vec();
        let mut bad = textures[channel].clone();
        if let SceneTexture::Texels { width, bytes, .. } = &mut bad {
            *width = 2;
            *bytes = [bytes.as_ref(), bytes.as_ref()].concat().into();
        }
        textures.push(bad);
        materials.push(if channel == 2 {
            material(Some(4), Some(3), 0, 0)
        } else {
            material(Some(2), Some(4), 0, 0)
        });
        let scene = scene
            .with_resources(
                SceneResources::new(textures, materials).unwrap(),
                scene.geometry().clone(),
                Some(effects()),
            )
            .unwrap();
        let mut first = decal();
        first.material = MaterialHandle::from_raw(1);
        let mut second = first;
        second.material = MaterialHandle::from_raw(2);
        assert!(
            crate::draw::validate_decal_materials(&scene.with_decals(vec![first, second]).unwrap())
                .is_err()
        );
    }
}

#[test]
fn fractional_normal_and_packed_surface_blends_match_baked_receivers() {
    let context = context();
    for slope in [false, true] {
        let scene = mapped_scene(slope);
        let mut textures = scene.resources().textures().to_vec();
        textures[1] = texel([51, 102, 153, 128]);
        let scene = scene
            .with_resources(
                SceneResources::new(textures, scene.resources().materials().to_vec()).unwrap(),
                scene.geometry().clone(),
                Some(effects()),
            )
            .unwrap();
        let expected = baked_scene(&scene, slope, 128);
        let mut d = decal();
        d.dimensions_millimeters = [4000; 3];
        d.edge_fade_micro = 0;
        d.angle_cutoff_micro = 0;
        d.angle_fade_micro = 0;
        let actual = capture(&context, &scene.with_decals(vec![d]).unwrap(), 1);
        let expected = capture(&context, &expected, 1);
        let offset = (32 * 64 + 32) * 4;
        for (actual, expected) in actual[offset..offset + 3]
            .iter()
            .zip(&expected[offset..offset + 3])
        {
            assert!(actual.abs_diff(*expected) <= 1, "{actual} != {expected}");
        }
    }
}
