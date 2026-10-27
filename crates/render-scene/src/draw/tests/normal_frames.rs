#[test]
fn terrain_palettes_refuse_unimplemented_normal_settings_before_upload() {
    let terrain_material = terrain_material();
    let base = terrain_scene(1);
    for material in [
        terrain_material.with_normal_scale(2_000_000).unwrap(),
        terrain_material.with_normal_encoding(SceneNormalEncoding::Rgb, SceneNormalY::Up),
        terrain_material
            .with_normal_encoding(SceneNormalEncoding::TwoChannelXy, SceneNormalY::Down),
    ] {
        let scene = RenderScene::new(SceneParts {
            resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
                .unwrap(),
            geometry: base.geometry().clone(),
            terrain: base.terrain().cloned(),
            effects: base.effects().cloned(),
        })
        .unwrap();
        assert!(matches!(
            super::super::PreparedScene::new(&scene),
            Err(DrawError::Frame(
                crate::frame::FrameError::UnsupportedEffect("terrain material normal settings")
            ))
        ));
    }
}

#[test]
fn absent_tangents_and_default_normal_settings_match_stock_shading_bytes() {
    let stock = concat!(
        worldrunner_render_lighting::lighting_wgsl!(),
        worldrunner_render_post::post_wgsl!(),
        include_str!("fixtures/stock_lighting.wgsl"),
        include_str!("../../shader/unshadowed.wgsl"),
        include_str!("fixtures/stock_environment.wgsl"),
    );
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let scene = normal_scene([191, 159, 7, 255], None, &mapped_material());
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
    gpu.set_frame(
        context.queue(),
        FrameUniform {
            background_far_plane_meters: 0.0,
            sun_irradiance: Vec3::ZERO,
            ambient_sky: Vec3::splat(0.4),
            ambient_ground: Vec3::splat(0.1),
            ..frame_from(pose())
        },
    );
    let normal = decoded_normal(
        [191, 159, 7, 255],
        SceneNormalEncoding::TwoChannelXy,
        SceneNormalY::Up,
        1_000_000,
    );
    let code = srgb(0.2 * (0.1 + 0.3 * (normal.z * 0.5 + 0.5))) * 255.0;
    assert!((code.fract() - 0.5).abs() >= 0.1);
    let expected = super::foliage::pass_color(&context, &gpu, &layouts, stock, "fs_opaque");
    assert!((f32::from(expected[(32 * TARGET as usize + 32) * 4]) - code).abs() < 0.5);
    assert_eq!(
        super::foliage::pass_color(
            &context,
            &gpu,
            &layouts,
            crate::shader::ENVIRONMENT_WGSL,
            "fs_opaque"
        ),
        expected
    );
}

use super::*;
use glam::{Mat4, Vec3};
use worldrunner_render_plan::{SceneNormalEncoding, SceneNormalY, SceneTint, TexelFormat};

fn texel(rgba: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Box::new(rgba),
    }
}

fn normal_scene(rgba: [u8; 4], tangent: Option<[i64; 4]>, mat: &SceneMaterial) -> RenderScene {
    let mut vertices = quad().lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.tangent_micro = tangent;
        if tangent.is_some_and(|t| t[2] != 0) {
            vertex.normal_micro = [MICRO, 0, MICRO];
        }
        vertex.texcoord_micro[0] = -vertex.texcoord_micro[0];
    }
    let lod = SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texel([51, 51, 51, 255]), texel(rgba)], vec![*mat])
            .unwrap(),
        geometry: SceneGeometry::new(
            vec![SceneMesh::new(vec![lod]).unwrap()],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

fn mapped_material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        normal: Some(TextureHandle::from_raw(1)),
        ..SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: None,
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 1_000_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: true,
        }
    })
    .unwrap()
}

fn diagnostic(source: &str) -> String {
    source.replace(
        "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);",
        "return vec4<f32>(normal * 0.2 + 0.4, 1.0);",
    ).replace("vec4<f32>(normal, 1.0)", "vec4<f32>(normal * 0.2 + 0.4, 1.0)")
        .replace("vec4<f32>(normal, roughness)", "vec4<f32>(normal * 0.2 + 0.4, 1.0)")
}

fn srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn assert_normal(pixels: &[u8], normal: Vec3) {
    let offset = (32 * TARGET as usize + 32) * 4;
    for (channel, expected) in normal.to_array().into_iter().enumerate() {
        let code = srgb(expected * 0.2 + 0.4) * 255.0;
        assert!(
            (f32::from(pixels[offset + channel]) - code).abs() <= 1.0,
            "normal={normal:?}, channel={channel}, actual={}, expected={code}",
            pixels[offset + channel]
        );
    }
}

#[test]
fn stored_tangents_follow_handedness_mirrored_uv_and_affine_scale_in_all_normal_passes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let xy = Vec3::new(191.0 / 255.0 * 2.0 - 1.0, 159.0 / 255.0 * 2.0 - 1.0, 0.0);
    let z = (1.0 - xy.x * xy.x - xy.y * xy.y).sqrt();
    for scale in [
        Vec3::ONE,
        Vec3::new(2.0, 3.0, 4.0),
        Vec3::new(-2.0, 3.0, 4.0),
    ] {
        for handedness in [-1, 1] {
            let scene = normal_scene(
                [191, 159, 7, 255],
                Some([MICRO, 0, -MICRO, handedness * MICRO]),
                &mapped_material(),
            );
            let prepared = super::super::PreparedScene::new(&scene).unwrap();
            let gpu = SceneGpu::upload_prepared(
                context.device(),
                context.submission(),
                prepared,
                &layouts,
            )
            .unwrap();
            gpu.set_frame(context.queue(), frame_from(pose()));
            let model = crate::geometry::pack_model_instance(Mat4::from_scale(scale));
            let batch = gpu.instances.batches.values().next().unwrap();
            context
                .queue()
                .write_buffer(batch.buffer.as_ref().unwrap(), 0, &model);
            let n = Vec3::new(scale.x.recip(), 0.0, scale.z.recip()).normalize() * scale.x.signum();
            let t = Vec3::new(scale.x, 0.0, -scale.z).normalize();
            let sign = if handedness < 0 { -1.0 } else { 1.0 };
            let b = n.cross(t) * sign * scale.x.signum();
            assert_pass_normals(
                &context,
                &gpu,
                &layouts,
                (t * xy.x + b * xy.y + n * z).normalize(),
            );
        }
    }
}

#[test]
fn source_channels_y_conventions_and_normal_scale_reach_every_normal_pass() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for encoding in [SceneNormalEncoding::Rgb, SceneNormalEncoding::TwoChannelXy] {
        for y in [SceneNormalY::Up, SceneNormalY::Down] {
            for rgba in [[191, 159, 231, 255], [255, 255, 231, 255]] {
                for scale in [0, 1_000_000, 2_000_000] {
                    let material = mapped_material()
                        .with_normal_encoding(encoding, y)
                        .with_normal_scale(scale)
                        .unwrap();
                    let scene = normal_scene(rgba, Some([MICRO, 0, 0, MICRO]), &material);
                    let gpu =
                        SceneGpu::upload(context.device(), context.submission(), &scene, &layouts)
                            .unwrap();
                    gpu.set_frame(context.queue(), frame_from(pose()));
                    assert_pass_normals(
                        &context,
                        &gpu,
                        &layouts,
                        decoded_normal(rgba, encoding, y, scale),
                    );
                }
            }
        }
    }
}

#[test]
fn brightening_tint_reaches_shaded_output_linearly() {
    let context = context();
    let material = mapped_material().with_normal_scale(0).unwrap();
    let scene = normal_scene([128, 128, 255, 255], None, &material);
    let frame = FrameUniform {
        background_far_plane_meters: 0.0,
        sun_irradiance: Vec3::ZERO,
        ambient_sky: Vec3::splat(0.4),
        ambient_ground: Vec3::splat(0.4),
        ..frame_from(pose())
    };
    let mut values = Vec::new();
    for factor in [1_000_000, 2_000_000, 2_550_000] {
        let object = scene.geometry().objects()[0].with_tint(SceneTint::new([factor; 3]).unwrap());
        let tinted = RenderScene::new(SceneParts {
            resources: scene.resources().clone(),
            geometry: SceneGeometry::new(scene.geometry().meshes().to_vec(), vec![object]).unwrap(),
            terrain: None,
            effects: scene.effects().cloned(),
        })
        .unwrap();
        let pixels = render_frame(&context, &tinted, &frame);
        let actual = pixels[(32 * TARGET as usize + 32) * 4];
        let factor = match factor {
            1_000_000 => 1.0,
            2_000_000 => 2.0,
            _ => 2.55,
        };
        let code = srgb(0.08 * factor) * 255.0;
        assert!((code.fract() - 0.5).abs() >= 0.1);
        assert!(
            (f32::from(actual) - code).abs() < 0.5,
            "actual={actual}, expected={code}"
        );
        values.push(f32::from(actual));
    }
    assert!(
        values[1] > values[0] + 10.0 && values[2] > values[1] + 5.0,
        "{values:?}"
    );
}

fn assert_pass_normals(
    context: &GpuContext,
    gpu: &SceneGpu,
    layouts: &ScenePipelines,
    normal: Vec3,
) {
    for source in super::foliage::geometry_sources().iter().take(3) {
        assert_normal(
            &super::foliage::pass_color(context, gpu, layouts, &diagnostic(source), "fs_opaque"),
            normal,
        );
    }
}

fn decoded_normal(
    rgba: [u8; 4],
    encoding: SceneNormalEncoding,
    y: SceneNormalY,
    scale: u32,
) -> Vec3 {
    if scale == 0 {
        return Vec3::Z;
    }
    let decoded = rgba.map(|v| f32::from(v) / 255.0 * 2.0 - 1.0);
    let z = if encoding == SceneNormalEncoding::Rgb {
        decoded[2]
    } else {
        (1.0 - decoded[0].powi(2) - decoded[1].powi(2))
            .max(0.0)
            .sqrt()
    };
    let factor = f32::from(u16::try_from(scale / 1_000_000).unwrap());
    Vec3::new(
        decoded[0] * factor,
        decoded[1] * factor * if y == SceneNormalY::Up { 1.0 } else { -1.0 },
        z,
    )
    .normalize()
}

#[test]
fn sh9_diffuse_produces_the_same_radiance_for_mesh_terrain_and_cards() {
    use worldrunner_render_plan::{SceneDiffuseEnvironment, SceneSh9};
    let context = context();
    let mut coefficients = [[0; 3]; 9];
    coefficients[0] = [250_000, 375_000, 125_000];
    let frame = FrameUniform {
        background_far_plane_meters: 0.0,
        specular_convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        diffuse_environment: SceneDiffuseEnvironment::Sh9(SceneSh9::new(coefficients).unwrap()),
        sun_irradiance: Vec3::ZERO,
        ..frame_from(pose())
    };
    let expected = [0.25, 0.375, 0.125].map(|linear| srgb(linear) * 255.0);
    for code in expected {
        assert!((code.fract() - 0.5).abs() >= 0.1);
    }
    let scenes = [
        scene(SceneAlphaMode::Opaque, false, 1),
        terrain_scene(1),
        card_scene(Some(BillboardPolicy::YawLocked), identity().rotation),
    ];
    for source in scenes {
        let pixels = render_frame(&context, &source, &frame);
        let covered: Vec<_> = pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] != 0)
            .collect();
        assert!(!covered.is_empty());
        for pixel in covered {
            for (channel, code) in expected.iter().enumerate() {
                assert!(
                    (f32::from(pixel[channel]) - code).abs() < 0.5,
                    "channel {channel}: {} vs {code}",
                    pixel[channel]
                );
            }
        }
    }
}
