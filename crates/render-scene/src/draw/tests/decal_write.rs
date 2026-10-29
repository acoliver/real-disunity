use super::{capture, decal, source, texel};
use crate::draw::tests::{context, effects};
use worldrunner_render_plan::{
    MaterialHandle, RenderScene, SceneAlphaMode, SceneDecal, SceneDecalWriteMask, SceneGeometry,
    SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject, SceneResources,
    SceneTerrainContact, SceneTint, TextureHandle,
};

fn material(albedo: u32, normal: Option<u32>, rm: Option<u32>, r: u32, m: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal: normal.map(TextureHandle::from_raw),
        roughness_metallic: rm.map(TextureHandle::from_raw),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: r,
        metalness_micro: m,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

fn metadata(
    context: &crate::device::GpuContext,
    scene: &worldrunner_render_plan::RenderScene,
    material: bool,
) -> Vec<u8> {
    let mut parts = crate::draw::tests::effect_parts();
    parts.ssao = Some(crate::ssr::neutral_ao());
    let effects = worldrunner_render_plan::SceneEffects::new(parts).unwrap();
    let scene = scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    crate::draw::tests::ssao::capture_metadata(context, &scene, 1, material).1
}

#[test]
fn albedo_only_preserves_receiver_normal_and_surface_before_lighting() {
    let context = context();
    for terrain in [false, true] {
        let base = source(false, terrain);
        let mut textures = base.resources().textures().to_vec();
        textures[1] = texel([204, 51, 26, 255]);
        textures.push(worldrunner_render_plan::SceneTexture::Texels {
            format: worldrunner_render_plan::TexelFormat::Rg8Unorm,
            color_space: worldrunner_render_plan::TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![128, 191].into(),
        });
        textures.push(texel([51, 249, 204, 255]));
        let receiver = material(0, Some(2), Some(3), 700_000, 100_000);
        let projection = material(1, Some(4), Some(5), 200_000, 800_000);
        let resources = SceneResources::new(textures.clone(), vec![receiver, projection]).unwrap();
        let scene = base
            .with_resources(resources, base.geometry().clone(), Some(effects()))
            .unwrap();
        let mut d = decal();
        d.dimensions_millimeters = [4000; 3];
        d.edge_fade_micro = 0;
        d.write_mask = SceneDecalWriteMask::ALBEDO;
        let projected = scene.with_decals(vec![d]).unwrap();
        let expected = base
            .with_resources(
                SceneResources::new(
                    textures,
                    vec![material(1, Some(2), Some(3), 700_000, 100_000), projection],
                )
                .unwrap(),
                base.geometry().clone(),
                Some(effects()),
            )
            .unwrap();
        for material in [false, true] {
            assert_eq!(
                metadata(&context, &projected, material),
                metadata(&context, &expected, material)
            );
        }
        let actual = capture(&context, &projected, 1);
        let expected = capture(&context, &expected, 1);
        for (x, y) in [(32, 32), (28, 32), (36, 32)] {
            let offset = (y * 64 + x) * 4;
            assert_eq!(
                &actual[offset..offset + 4],
                &expected[offset..offset + 4],
                "terrain={terrain}"
            );
        }
    }
}

#[test]
fn alpha_test_below_at_above_cutoff_and_defaults_preserve_bytes() {
    let context = context();
    let scene = source(false, false);
    let mut d = decal();
    d.dimensions_millimeters = [4000; 3];
    d.edge_fade_micro = 0;
    let original = capture(&context, &scene, 1);
    let baseline = capture(&context, &scene.with_decals(vec![d]).unwrap(), 1);
    assert_ne!(baseline, original);
    for (cutoff, accepted) in [(501_960, true), (501_961, false), (0, true)] {
        d.alpha_test_cutoff_micro = Some(cutoff);
        assert_eq!(
            capture(&context, &scene.with_decals(vec![d]).unwrap(), 1),
            if accepted {
                baseline.clone()
            } else {
                original.clone()
            }
        );
    }
    d.alpha_test_cutoff_micro = None;
    d.write_mask = SceneDecalWriteMask::default();
    assert_eq!(
        capture(&context, &scene.with_decals(vec![d]).unwrap(), 1),
        baseline
    );
}

#[test]
fn opaque_alpha_passes_equal_cutoff_but_angle_fade_is_tested_first() {
    let context = context();
    for slope in [false, true] {
        let base = source(slope, false);
        let mut textures = base.resources().textures().to_vec();
        textures[1] = texel([204, 51, 26, 255]);
        let mut materials = base.resources().materials().to_vec();
        materials[1] = materials[1].with_alpha_factor(250_000).unwrap();
        let scene = base
            .with_resources(
                SceneResources::new(textures, materials).unwrap(),
                base.geometry().clone(),
                None,
            )
            .unwrap();
        let mut d = decal();
        d.edge_fade_micro = 500_000;
        d.dimensions_millimeters = [4000; 3];
        d.mapping.angle = worldrunner_render_plan::SceneDecalAngle::Power {
            threshold_micro: 1_000_000,
            exponent_micro: 2_000_000,
        };
        let original = capture(&context, &scene, 1);
        let blended = capture(&context, &scene.with_decals(vec![d]).unwrap(), 1);
        d.alpha_test_cutoff_micro = Some(1_000_000);
        let actual = capture(&context, &scene.with_decals(vec![d]).unwrap(), 1);
        assert_eq!(actual, if slope { original } else { blended });
    }
}

#[test]
fn default_uniform_bytes_match_the_existing_volume_layout() {
    use crate::draw::SceneGpu;
    use crate::draw::tests::{frame_from, pose};
    use crate::renderer::ScenePipelines;
    let context = context();
    let scene = source(false, false).with_decals(vec![decal()]).unwrap();
    let pipelines = ScenePipelines::new(context.device(), super::super::COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let actual = gpu.decals.prepare(&frame_from(pose()), &gpu.frame_buffer);
    let mut values = vec![f32::from_bits(1), 0.0, 0.0, 0.0];
    values.extend(
        glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.2, 1.0))
            .inverse()
            .to_cols_array(),
    );
    values.extend([0.0, 0.0, 1.0, 0.2, 0.2, 0.8, 0.6, 0.4]);
    values.extend([f32::from_bits(3), 0.0, f32::from_bits(2), 0.0]);
    values.extend([1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    let mut expected: Vec<_> = values.into_iter().flat_map(f32::to_le_bytes).collect();
    expected.resize(16 + 32 * 144, 0);
    assert_eq!(actual, expected);
}

fn folded_receiver(alpha: u8) -> RenderScene {
    use crate::draw::tests::identity;
    let base = crate::draw::tests::fold_translucency::folded_scene(0);
    let contact = SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 0)
        .unwrap()
        .with_normal_fold(2000, 500_000)
        .unwrap();
    let normal = worldrunner_render_plan::SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rg8Unorm,
        color_space: worldrunner_render_plan::TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128, 191].into(),
    };
    base.with_resources(
        SceneResources::new(
            vec![
                texel([51, 102, 153, 255]),
                texel([204, 51, 26, alpha]),
                texel([153, 204, 102, 255]),
                normal,
                texel([51, 249, 204, 255]),
                texel([26, 204, 51, 170]),
                texel([230, 1, 26, 255]),
                texel([128, 128, 255, 255]),
            ],
            vec![
                material(0, None, Some(2), 750_000, 500_000).with_terrain_contact(contact),
                material(0, Some(7), Some(2), 1_000_000, 0),
                material(1, Some(3), Some(4), 900_000, 100_000),
                material(5, Some(3), Some(6), 100_000, 900_000),
            ],
        )
        .unwrap(),
        SceneGeometry::new(
            base.geometry().meshes().to_vec(),
            vec![SceneObject::new(
                identity(),
                worldrunner_render_plan::MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        Some(effects()),
    )
    .unwrap()
}

fn interior_decal(mask: SceneDecalWriteMask, material: u32) -> SceneDecal {
    SceneDecal {
        dimensions_millimeters: [4000; 3],
        edge_fade_micro: 0,
        angle_cutoff_micro: 0,
        angle_fade_micro: 0,
        material: MaterialHandle::from_raw(material),
        write_mask: mask,
        ..decal()
    }
}

fn baked_receiver(
    scene: &RenderScene,
    albedo: [u8; 3],
    surface: [u32; 2],
    write_normal: bool,
) -> RenderScene {
    let mut textures = scene.resources().textures().to_vec();
    textures[0] = texel([albedo[0], albedo[1], albedo[2], 255]);
    let mut materials = scene.resources().materials().to_vec();
    let mut receiver = material(0, None, None, surface[0], surface[1]);
    let mut geometry = scene.geometry().clone();
    if write_normal {
        // Independently bake the decoded projection normal into receiver vertices.
        let x = 1.0_f32 / 255.0;
        let y = 127.0_f32 / 255.0;
        let normal = glam::Vec3::new(y, -x, (1.0 - x * x - y * y).sqrt());
        let normal = (normal * 1_000_000.0).round().as_ivec3().to_array();
        let mut vertices = geometry.meshes()[0].lods()[0].vertices().to_vec();
        for vertex in &mut vertices {
            vertex.normal_micro = normal.map(i64::from);
        }
        let mesh = SceneMesh::new(vec![
            SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
        ])
        .unwrap();
        geometry = SceneGeometry::new(vec![mesh], geometry.objects().to_vec()).unwrap();
    } else {
        receiver = receiver.with_terrain_contact(
            SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 0)
                .unwrap()
                .with_normal_fold(2000, 500_000)
                .unwrap(),
        );
    }
    materials[0] = receiver;
    scene
        .with_resources(
            SceneResources::new(textures, materials).unwrap(),
            geometry,
            Some(effects()),
        )
        .unwrap()
}

fn products(context: &crate::device::GpuContext, scene: &RenderScene) -> [Vec<u8>; 3] {
    [
        capture(context, scene, 1),
        metadata(context, scene, false),
        metadata(context, scene, true),
    ]
}

fn half_pixel(bytes: &[u8], x: usize, y: usize) -> [f32; 4] {
    let offset = (y * 65 + x) * 8;
    std::array::from_fn(|channel| {
        let start = offset + channel * 2;
        crate::ssao::tests::decode_half(u16::from_le_bytes(
            bytes[start..start + 2].try_into().unwrap(),
        ))
    })
}

fn assert_interior_products(actual: &[Vec<u8>; 3], expected: &[Vec<u8>; 3]) {
    for x in [28, 32, 36] {
        let offset = (32 * 64 + x) * 4;
        for channel in 0..4 {
            assert!(
                actual[0][offset + channel].abs_diff(expected[0][offset + channel]) <= 1,
                "forward x={x} channel={channel}: {} != {}",
                actual[0][offset + channel],
                expected[0][offset + channel]
            );
        }
        for product in [1, 2] {
            for (actual, expected) in half_pixel(&actual[product], x, 24)
                .into_iter()
                .zip(half_pixel(&expected[product], x, 24))
            {
                assert!(
                    (actual - expected).abs() <= 0.001,
                    "metadata product={product} x={x}: {actual} != {expected}"
                );
            }
        }
    }
}

fn assert_partial_fold(normals: &[u8]) {
    let pixel = half_pixel(normals, 32, 24);
    let magnitude = glam::Vec3::from_array(pixel[..3].try_into().unwrap()).length();
    // Halfway between (-0.6, 0, 0.8) and +Z, without normalization.
    assert!((magnitude - 0.9_f32.sqrt()).abs() < 0.001, "{pixel:?}");
}

const MASK_CASES: [(SceneDecalWriteMask, [u8; 3], [u32; 2], bool); 5] = [
    (
        SceneDecalWriteMask::NONE,
        [51, 102, 153],
        [600_000, 200_000],
        false,
    ),
    (
        SceneDecalWriteMask::ALBEDO,
        [204, 51, 26],
        [600_000, 200_000],
        false,
    ),
    (
        SceneDecalWriteMask::NORMAL,
        [51, 102, 153],
        [600_000, 200_000],
        true,
    ),
    (
        SceneDecalWriteMask::ROUGHNESS_METALNESS,
        [51, 102, 153],
        [200_000, 800_000],
        false,
    ),
    (
        SceneDecalWriteMask::from_bits(6),
        [51, 102, 153],
        [200_000, 800_000],
        true,
    ),
];

#[test]
fn individual_masks_and_disabled_albedo_match_baked_folded_receivers() {
    let context = context();
    let base = folded_receiver(255);
    let receiver = products(&context, &base);
    assert_partial_fold(&receiver[1]);
    for (mask, albedo, surface, normal) in MASK_CASES {
        let scene = base.with_decals(vec![interior_decal(mask, 2)]).unwrap();
        let actual = products(&context, &scene);
        let expected = products(&context, &baked_receiver(&base, albedo, surface, normal));
        assert_interior_products(&actual, &expected);
        if !normal {
            assert_partial_fold(&actual[1]);
        }
        if mask == SceneDecalWriteMask::NONE {
            assert_eq!(actual, receiver);
        }
    }
}

#[test]
fn overlapping_fractional_mixed_masks_follow_both_caller_orders() {
    let context = context();
    let base = folded_receiver(85);
    let first = interior_decal(SceneDecalWriteMask::from_bits(5), 2);
    let second = interior_decal(SceneDecalWriteMask::ROUGHNESS_METALNESS, 3);
    let mut outputs = Vec::new();
    for reverse in [false, true] {
        let order = if reverse { [1, 0] } else { [0, 1] };
        let mut surface = [600_000_u64, 200_000];
        for index in order {
            let (target, alpha) = [
                ([200_000, 800_000], 85),
                ([230 * 1_000_000 / 255, 26 * 1_000_000 / 255], 170),
            ][index];
            for channel in 0..2 {
                surface[channel] =
                    (surface[channel] * (255 - alpha) + target[channel] * alpha + 127) / 255;
            }
        }
        let expected = baked_receiver(
            &base,
            [102, 85, 111],
            surface.map(|value| u32::try_from(value).unwrap()),
            false,
        );
        let decals = [first, second];
        let scene = base
            .with_decals(order.map(|index| decals[index]).to_vec())
            .unwrap();
        let actual = products(&context, &scene);
        assert_interior_products(&actual, &products(&context, &expected));
        assert_partial_fold(&actual[1]);
        outputs.push(actual);
    }
    assert_ne!(
        &outputs[0][2][(24 * 65 + 32) * 8..(24 * 65 + 33) * 8],
        &outputs[1][2][(24 * 65 + 32) * 8..(24 * 65 + 33) * 8]
    );
    assert_ne!(
        &outputs[0][0][(32 * 64 + 32) * 4..(32 * 64 + 33) * 4],
        &outputs[1][0][(32 * 64 + 32) * 4..(32 * 64 + 33) * 4]
    );
}

#[test]
fn alpha_rejection_preserves_forward_normal_and_material_with_active_maps() {
    let context = context();
    let base = folded_receiver(128);
    let receiver = products(&context, &base);
    assert_partial_fold(&receiver[1]);
    let mut d = interior_decal(SceneDecalWriteMask::ALL, 2);
    d.alpha_test_cutoff_micro = Some(750_000);
    assert_eq!(
        products(&context, &base.with_decals(vec![d]).unwrap()),
        receiver
    );
    d.alpha_test_cutoff_micro = Some(500_000);
    let accepted = products(&context, &base.with_decals(vec![d]).unwrap());
    let center = (32 * 64 + 32) * 4;
    assert_ne!(
        &accepted[0][center..center + 4],
        &receiver[0][center..center + 4]
    );
    for product in [1, 2] {
        assert_ne!(
            &accepted[product][(24 * 65 + 32) * 8..(24 * 65 + 33) * 8],
            &receiver[product][(24 * 65 + 32) * 8..(24 * 65 + 33) * 8]
        );
    }
}
