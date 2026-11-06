//! Synthetic packed cards with adjacent alpha holes, varying mip data and mapped decals.
use super::*;
use worldrunner_render_plan::{
    SceneAtlasSampling, SceneFadeBand, SceneFadeCoverage, SceneFadeDirection,
    SceneFadeQuantization, SceneImpostor, SceneImpostorChannels, SceneImpostorLayout, TexelFormat,
};

fn atlas(channel: usize) -> SceneTexture {
    let mut bytes = Vec::new();
    for level in 0..5_u32 {
        let size = 48 >> level;
        let cell = size / 3;
        for y in 0..size {
            for x in 0..size {
                let frame = u8::try_from(x / cell + y / cell * 3).unwrap();
                let hole = (x % cell + y % cell) % 4 < 2;
                let alpha = if hole { 0 } else { 127 + frame * 13 };
                let value = match channel {
                    0 => [60 + frame * 17, 150, 210 - frame * 11, alpha],
                    1 => [112 + frame * 3, 125 + frame, 240, 255],
                    2 => [80 + frame * 16 + u8::try_from(level).unwrap(), 0, 0, 255],
                    3 => [40 + frame * 21, 5 + frame * 2, 110, 255],
                    4 => [0, 190, 0, 255],
                    _ => unreachable!(),
                };
                bytes.extend(value);
            }
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 48,
        height: 48,
        mips: 5,
        bytes: bytes.into(),
    }
}

pub(super) const A2C_FIXTURE: &str = "masked alpha-varying packed impostor with A2C";

fn resources(cutoff: u32, factor: u32, fade: usize, a2c: bool) -> SceneResources {
    let mut textures: Vec<_> = (0..5).map(atlas).collect();
    textures.extend([
        decals::texel([180, 60, 110, 171]),
        SceneTexture::Texels {
            format: TexelFormat::Rg8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![150, 177].into(),
        },
        decals::texel([77, 191, 133, 255]),
        SceneTexture::Texels {
            format: TexelFormat::R8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 2,
            height: 2,
            mips: 1,
            bytes: vec![0, 119, 120, 255].into(),
        },
    ]);
    let impostor = SceneImpostor::new(
        SceneImpostorLayout::HemisphericalOctahedral,
        [3, 3],
        12_345,
        true,
    )
    .unwrap()
    .with_plane_size([2000, 2000])
    .unwrap()
    .with_relief(TextureHandle::from_raw(2), 310)
    .unwrap()
    .with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion)
    .with_sampling(SceneAtlasSampling::Implicit)
    .with_pass_cutoffs(cutoff, 317_003)
    .unwrap();
    let base = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(3)),
        ambient_occlusion: Some(TextureHandle::from_raw(4)),
        self_illumination: None,
        roughness_micro: 719_003,
        metalness_micro: 123_457,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 271_003,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(impostor)
    .with_alpha_factor(factor)
    .unwrap()
    .with_alpha_to_coverage(a2c)
    .unwrap();
    let base = if fade >= 3 {
        base.with_fade_coverage(SceneFadeCoverage::Threshold {
            texture: TextureHandle::from_raw(8),
            jitter_pixels: [1, -2],
        })
        .with_fade_quantization(SceneFadeQuantization::Byte)
    } else {
        base
    };
    let base = if fade == 4 || fade == 6 {
        base.with_fade_direction(SceneFadeDirection::Complement)
    } else {
        base
    };
    let projected = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(5),
        normal: Some(TextureHandle::from_raw(6)),
        roughness_metallic: Some(TextureHandle::from_raw(7)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 700_000,
        metalness_micro: 900_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    SceneResources::new(textures, vec![base, projected]).unwrap()
}

fn cards(fade: usize, back: bool) -> SceneGeometry {
    let card = card(Some(BillboardPolicy::YawLocked));
    let lod = &card.lods()[0];
    let indices = if back {
        vec![2, 1, 0, 3, 2, 0]
    } else {
        vec![0, 1, 2, 0, 2, 3]
    };
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            lod.vertices().to_vec(),
            indices,
            Some(BillboardPolicy::YawLocked),
        )
        .unwrap(),
    ])
    .unwrap();
    let band = match fade {
        0 => [6000, 7000],
        1 => [1000, 2000],
        5 | 6 => [4000, 4000],
        _ => [3000, 6000],
    };
    let positions = if fade >= 5 { vec![0] } else { vec![-160, 160] };
    let objects = positions
        .into_iter()
        .map(|x| {
            let mut transform = identity();
            transform.position_millimeters = [x, -x, 0];
            SceneObject::new(
                transform,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )
            .with_fade(SceneFadeBand::new(band[0], band[1]).unwrap())
        })
        .collect();
    SceneGeometry::new(vec![mesh], objects).unwrap()
}

pub(super) fn scenes() -> Vec<(&'static str, RenderScene, CameraPose)> {
    let mut result = Vec::new();
    for (cutoff, factor, fade, back, x, a2c) in [
        (317_003, 791_003, 1, false, 3999, false),
        (317_003, 791_003, 1, true, 4001, false),
        (5003, 500_000, 2, false, 4000, false),
        (277_003, 633_337, 0, true, 0, false),
        (277_003, 633_337, 3, false, 0, false),
        (277_003, 633_337, 4, true, 0, false),
        (277_003, 633_337, 5, false, 0, false),
        (277_003, 633_337, 6, true, 0, false),
        (317_003, 791_003, 1, false, 3999, true),
    ] {
        let mut decal = embedded_decal();
        decal.dimensions_millimeters = [6000; 3];
        decal.axis = worldrunner_render_plan::SceneDecalAxis::Y;
        decal.mapping.double_sided = true;
        decal.angle_cutoff_micro = 0;
        decal.angle_fade_micro = 0;
        let scene = RenderScene::new(SceneParts {
            resources: resources(cutoff, factor, fade, a2c),
            geometry: cards(fade, back),
            terrain: None,
            effects: None,
        })
        .unwrap()
        .with_decals(vec![decal])
        .unwrap();
        result.push((
            match (fade, back) {
                (1, _) if a2c => A2C_FIXTURE,
                (0, _) => "zero hashed fade",
                (1, false) => "packed frame-cell left/front overlap",
                (1, true) => "packed frame-cell right/back overlap",
                (2, _) => "packed frame-cell boundary odd cutoff",
                (3, _) => "byte threshold fade",
                (4, _) => "complementary byte threshold fade",
                (5, _) => "zero step fade preserves external zero threshold",
                (6, _) => "complementary step fade boundary",
                _ => unreachable!(),
            },
            scene,
            CameraPose {
                eye_millimeters: [x, -4000, if fade >= 5 { 0 } else { 2000 }],
                target_millimeters: [0, 0, 1000],
            },
        ));
    }
    result
}

#[test]
fn masked_impostor_fixture_specializes_alpha_to_coverage_at_four_samples_only() {
    let (_, scene, _) = scenes()
        .into_iter()
        .find(|(name, _, _)| *name == A2C_FIXTURE)
        .unwrap();
    let key = crate::pipeline::PipelineKey::from_material(&scene.resources().materials()[0]);
    assert!(key.masked && key.sampling.is_impostor());
    assert!(key.alpha_to_coverage(4));
    assert!(!key.alpha_to_coverage(1));
    assert_eq!(
        key.fragment_constants(4)[0],
        ("SCENE_ALPHA_TO_COVERAGE", 1.0)
    );
    assert_eq!(
        key.fragment_constants(1)[0],
        ("SCENE_ALPHA_TO_COVERAGE", 0.0)
    );
}

pub(super) fn assert_surviving_cutout_edges(depth: &[u8], normals: &[u8]) {
    let covered: Vec<_> = depth
        .chunks_exact(4)
        .map(|v| v != 1.0_f32.to_le_bytes())
        .collect();
    let width = SIZE[0] as usize;
    let mut edges = 0;
    for (y, row) in covered.chunks_exact(width).enumerate() {
        let (Some(first), Some(last)) = (row.iter().position(|v| *v), row.iter().rposition(|v| *v))
        else {
            continue;
        };
        for x in first..last {
            if row[x] != row[x + 1] {
                let survivor = y * width + x + usize::from(!row[x]);
                assert_ne!(&normals[survivor * 8 + 6..survivor * 8 + 8], &[0, 0]);
                edges += 1;
            }
        }
    }
    assert!(
        edges > 0,
        "A2C fixture must retain metadata beside interior alpha holes"
    );
}

pub(super) fn raw_depth(context: &GpuContext, depth: &wgpu::Texture) -> Vec<u8> {
    let device = context.device();
    let row = (SIZE[0] * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("raw metadata device depth"),
        size: u64::from(row * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: depth,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        depth.size(),
    );
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &buffer)
        .chunks_exact(row as usize)
        .flat_map(|bytes| bytes[..SIZE[0] as usize * 4].iter().copied())
        .collect()
}
