#[test]
fn octahedral_card_blends_three_independently_colored_cells() {
    let base = scene(false, SceneRotation::IDENTITY);
    let texture = octahedral_texture();
    let material = base.resources().materials()[0].with_impostor(
        SceneImpostor::new(
            SceneImpostorLayout::HemisphericalOctahedral,
            [3, 3],
            0,
            true,
        )
        .unwrap(),
    );
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material]).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &source, &layouts).unwrap();
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 2000],
        target_millimeters: [0, 0, 1000],
    });
    frame.unlit = true;
    gpu.set_frame(context.queue(), frame);
    let pixels = super::foliage::pass_color(
        &context,
        &gpu,
        &layouts,
        crate::shader::ENVIRONMENT_WGSL,
        "fs_opaque",
    );
    let center = ((TARGET / 2 * TARGET + TARGET / 2) * 4) as usize;
    // L1-normalized (0,-4,2) maps to (1/3,5/3) in a 3x3 grid.
    // The triangle has equal weights, yielding linear RGB (1/3,1/3,1/3).
    for actual in &pixels[center..center + 3] {
        assert!(
            i32::from(*actual) == 156,
            "{:?}",
            &pixels[center..center + 4]
        );
    }
}

#[test]
fn atlas_blending_interpolates_linear_color_on_a_card() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = half_color_scene();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &source, &layouts).unwrap();
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [4000, 4000, 1000],
        target_millimeters: [0, 0, 1000],
    });
    frame.unlit = true;
    gpu.set_frame(context.queue(), frame);
    let pixels = super::foliage::pass_color(
        &context,
        &gpu,
        &layouts,
        crate::shader::ENVIRONMENT_WGSL,
        "fs_opaque",
    );
    let center = ((TARGET / 2 * TARGET + TARGET / 2) * 4) as usize;
    // A 45-degree view mixes linearized sRGB(200) equally. The result
    // encodes to 146.313, more than 0.1 from either rounding boundary.
    for (actual, expected) in pixels[center..center + 3].iter().zip([146, 146, 0]) {
        assert!(
            i32::from(*actual) == expected,
            "{:?}",
            &pixels[center..center + 4]
        );
    }
}

use super::*;
use worldrunner_render_plan::{SceneImpostor, SceneImpostorLayout, TexelFormat};

fn scene(masked: bool, rotation: SceneRotation) -> RenderScene {
    let colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255; 4],
    ];
    let mut bytes = Vec::new();
    for y in 0..8 {
        for color in colors {
            for _ in 0..8 {
                let mut texel = color;
                if masked && y < 4 {
                    texel[3] = 0;
                }
                bytes.extend_from_slice(&texel);
            }
        }
    }
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 32,
        height: 8,
        mips: 1,
        bytes: bytes.into(),
    };
    let material = material(
        if masked {
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            }
        } else {
            SceneAlphaMode::Opaque
        },
        true,
    )
    .with_impostor(
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [4, 1], 0, true).unwrap(),
    );
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material]).unwrap(),
        geometry: SceneGeometry::new(
            vec![card(Some(BillboardPolicy::YawLocked))],
            vec![SceneObject::new(
                SceneTransform {
                    rotation,
                    ..identity()
                },
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn atlas_material_draws_selected_color_and_instance_orientation() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (rotation, expected) in [
        (SceneRotation::IDENTITY, [255, 255, 255]),
        (
            SceneRotation {
                x: 0,
                y: 0,
                z: 707_107,
                w: 707_107,
            },
            [255, 0, 0],
        ),
    ] {
        let source = scene(false, rotation);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &source, &layouts).unwrap();
        let mut frame = frame_from(CameraPose {
            eye_millimeters: [0, -4000, 1000],
            target_millimeters: [0, 0, 1000],
        });
        frame.unlit = true;
        gpu.set_frame(context.queue(), frame);
        let pixels = super::foliage::pass_color(
            &context,
            &gpu,
            &layouts,
            crate::shader::ENVIRONMENT_WGSL,
            "fs_opaque",
        );
        let center = ((TARGET / 2 * TARGET + TARGET / 2) * 4) as usize;
        for (actual, expected) in pixels[center..center + 3].iter().zip(expected) {
            assert!(
                i32::from(*actual).abs_diff(expected) <= 1,
                "{:?}",
                &pixels[center..center + 4]
            );
        }
    }
}

#[test]
fn atlas_cutout_depth_matches_forward_metadata() {
    let context = context();
    let device = context.device();
    let forward = ScenePipelines::new(device, COLOR_FORMAT);
    let source = relief_scene();
    let gpu = SceneGpu::upload(device, context.submission(), &source, &forward).unwrap();
    gpu.set_frame(
        context.queue(),
        frame_from(CameraPose {
            eye_millimeters: [0, -4000, 2000],
            target_millimeters: [0, 0, 1000],
        }),
    );
    let screen = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 224,
        usage: BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let (metadata, group) = crate::scene_metadata::pipelines(device, &screen, false);
    let (reflection, reflection_group) = crate::scene_metadata::pipelines(device, &screen, true);
    let read = super::curved_wind::read_depth;
    let expected = read(
        &context,
        &gpu,
        forward.object_pipelines(),
        (Some(COLOR_FORMAT), None),
    );
    let covered = expected
        .chunks_exact(4)
        .filter(|v| *v != 1.0_f32.to_le_bytes())
        .count();
    assert!(covered > 0 && covered < (TARGET * TARGET) as usize / 2);
    for (pipeline, state) in [
        (
            metadata.object_pipelines(),
            (Some(TextureFormat::Rgba16Float), Some(&group)),
        ),
        (
            reflection.object_pipelines(),
            (Some(TextureFormat::Rgba16Float), Some(&reflection_group)),
        ),
    ] {
        assert_eq!(expected, read(&context, &gpu, pipeline, state));
    }
}

#[test]
fn malformed_atlas_is_rejected_before_gpu_upload() {
    let base = scene(false, SceneRotation::IDENTITY);
    let mut materials = base.resources().materials().to_vec();
    materials[0] = materials[0].with_impostor(
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [5, 1], 0, true).unwrap(),
    );
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    assert!(matches!(
        super::super::PreparedScene::new(&source),
        Err(DrawError::Frame(
            crate::frame::FrameError::UnsupportedEffect("impostor atlas cell dimensions")
        ))
    ));
}

fn octahedral_texture() -> SceneTexture {
    let mut bytes = Vec::new();
    for y in 0..24 {
        for x in 0..24 {
            let color = match (x / 8, y / 8) {
                (0, 1) => [255, 0, 0, 255],
                (0, 2) => [0, 255, 0, 255],
                (1, 2) => [0, 0, 255, 255],
                _ => [0, 0, 0, 255],
            };
            bytes.extend_from_slice(&color);
        }
    }
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 24,
        height: 24,
        mips: 1,
        bytes: bytes.into(),
    }
}

pub(super) fn relief_scene() -> RenderScene {
    let base = scene(true, SceneRotation::IDENTITY);
    let mut textures = base.resources().textures().to_vec();
    let data = |value: [u8; 4]| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 32,
        height: 8,
        mips: 1,
        bytes: value.repeat(32 * 8).into(),
    };
    textures.push(data([128, 128, 0, 255]));
    textures.push(data([128; 4]));
    let mut parts = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap();
    let atlas = base.resources().materials()[0]
        .impostor()
        .unwrap()
        .with_relief(TextureHandle::from_raw(2), 100)
        .unwrap()
        .with_fade(worldrunner_render_plan::SceneFadeBand::new(3000, 6000).unwrap());
    parts = parts.with_impostor(atlas);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![parts]).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn half_color_scene() -> RenderScene {
    let base = scene(false, SceneRotation::IDENTITY);
    let mut textures = base.resources().textures().to_vec();
    let SceneTexture::Texels { bytes, .. } = &mut textures[0] else {
        unreachable!()
    };
    for texel in bytes.chunks_exact_mut(4) {
        for channel in &mut texel[..3] {
            if *channel == 255 {
                *channel = 200;
            }
        }
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn instance_fade_bands_share_forward_and_shadow_silhouettes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let shadow = crate::shadows::wind_test_pipelines(context.device(), &layouts);
    let base = relief_scene();
    for (band, visible) in [([5000, 6000], false), ([1000, 2000], true)] {
        let object = base.geometry().objects()[0]
            .with_fade(worldrunner_render_plan::SceneFadeBand::new(band[0], band[1]).unwrap());
        let scene = RenderScene::new(SceneParts {
            resources: base.resources().clone(),
            geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), vec![object]).unwrap(),
            terrain: None,
            effects: None,
        })
        .unwrap();
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        gpu.set_frame(
            context.queue(),
            frame_from(CameraPose {
                eye_millimeters: [0, -4000, 2000],
                target_millimeters: [0, 0, 1000],
            }),
        );
        let forward = super::curved_wind::read_depth(
            &context,
            &gpu,
            layouts.object_pipelines(),
            (Some(COLOR_FORMAT), None),
        );
        let depth = super::curved_wind::read_depth(&context, &gpu, &shadow, (None, None));
        assert_eq!(
            forward
                .chunks_exact(4)
                .map(|v| v != 1.0_f32.to_le_bytes())
                .collect::<Vec<_>>(),
            depth
                .chunks_exact(4)
                .map(|v| v != 1.0_f32.to_le_bytes())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            forward.chunks_exact(4).any(|v| v != 1.0_f32.to_le_bytes()),
            visible
        );
    }
}

fn contact_relief_scene() -> RenderScene {
    let base = relief_scene();
    let ground = super::foliage::contact_scene(0, false, false);
    let mut materials = base.resources().materials().to_vec();
    materials[0] = materials[0]
        .with_impostor(
            SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [4, 1], 0, true)
                .unwrap()
                .with_relief(TextureHandle::from_raw(2), 1000)
                .unwrap(),
        )
        .with_terrain_contact(
            worldrunner_render_plan::SceneTerrainContact::new(
                worldrunner_render_plan::SceneTint::new([0; 3]).unwrap(),
                1_000_000,
                0,
                3000,
            )
            .unwrap(),
        );
    materials.push(ground.resources().materials()[1]);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: ground.terrain().cloned(),
        effects: None,
    })
    .unwrap()
}

#[test]
fn metadata_contact_uses_relief_height_against_flat_ground_oracle() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = contact_relief_scene();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &source, &layouts).unwrap();
    let frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 2000],
        target_millimeters: [0, 0, 1000],
    });
    gpu.set_frame(context.queue(), frame);
    let shader = format!(
        "{}\n{}",
        crate::shader::ENVIRONMENT_WGSL
            .replace("fn fs_relief_opaque(", "fn unused_relief_opaque(")
            .replace("fn fs_relief_masked(", "fn unused_relief_masked(")
            .replace("fn fs_opaque(", "fn unused_opaque(")
            .replace("fn fs_masked(", "fn unused_masked("),
        include_str!("../../scene_metadata/geometry.wgsl").replace(
            "METADATA_OUTPUT",
            "vec4<f32>(vec3<f32>(contact_weight),1.0)"
        )
    );
    let pixels = super::foliage::pass_color(&context, &gpu, &layouts, &shader, "fs_opaque");
    let inverse = frame.view_projection.inverse();
    let pixel = TARGET / 2;
    let pixel_f = f32::from(u16::try_from(pixel).unwrap());
    let target_f = f32::from(u16::try_from(TARGET).unwrap());
    let ndc = glam::Vec3::new(
        2.0 * (pixel_f + 0.5) / target_f - 1.0,
        1.0 - 2.0 * (pixel_f + 0.5) / target_f,
        0.0,
    );
    let eye = glam::Vec3::new(0.0, -4.0, 2.0);
    let ray = inverse.project_point3(ndc) - eye;
    let card = eye + ray * (-eye.y / ray.y);
    let axis = glam::Vec3::new(0.0, 4.0, -1.0).normalize();
    let relieved = card - (card - eye) * ((128.0 / 255.0) / (card - eye).dot(axis));
    let weight = 1.0 - (relieved.z + 0.5) / 3.0;
    let encoded = (1.055 * weight.powf(1.0 / 2.4) - 0.055) * 255.0;
    let card_encoded = (1.055 * (1.0 - (card.z + 0.5) / 3.0).powf(1.0 / 2.4) - 0.055) * 255.0;
    assert!((card_encoded - encoded).abs() > 2.0);
    let center = ((pixel * TARGET + pixel) * 4) as usize;
    assert!(
        (f32::from(pixels[center]) - encoded).abs() <= 1.0,
        "{} vs {encoded}",
        pixels[center]
    );
}

#[test]
fn relief_raster_bias_keeps_surface_when_card_is_beyond_far_plane() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let base = relief_scene();
    let mut materials = base.resources().materials().to_vec();
    materials[0] = materials[0].with_impostor(
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [4, 1], 0, true)
            .unwrap()
            .with_relief(TextureHandle::from_raw(2), 3000)
            .unwrap(),
    );
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &source, &layouts).unwrap();
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 1000],
        target_millimeters: [0, 0, 1000],
    });
    frame.view_projection =
        glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, 1.0, 3.0)
            * frame.view;
    gpu.set_frame(context.queue(), frame);
    let bytes = super::curved_wind::read_depth(
        &context,
        &gpu,
        layouts.object_pipelines(),
        (Some(COLOR_FORMAT), None),
    );
    let center = ((TARGET / 2 * TARGET + TARGET / 2) * 4) as usize;
    let depth = f32::from_le_bytes(bytes[center..center + 4].try_into().unwrap());
    // Card depth is four metres, outside far=3. Relief gives 4-3*128/255.
    let distance = 4.0 - 3.0 * 128.0 / 255.0;
    let expected = 1.5 - 1.5 / distance;
    assert!((depth - expected).abs() < 0.00001, "{depth} vs {expected}");
}
