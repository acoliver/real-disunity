use super::foliage::{contact_scene, geometry_sources, pass_color};
use super::*;
use worldrunner_render_plan::{
    SceneTerrainContact, SceneTint, SceneTranslucency, SceneTranslucencyParts,
};

pub(super) fn folded_scene(height: i64) -> RenderScene {
    folded_placement(height, 0, None)
}
fn folded_placement(height: i64, minimum: u32, placement: Option<SceneTransform>) -> RenderScene {
    let base = contact_scene(-500, false, false);
    let mut vertices = quad().lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.position_millimeters[2] = height;
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap();
    let contact = SceneTerrainContact::new(SceneTint::WHITE, 1_000_000, 0, 1)
        .unwrap()
        .with_normal_fold(2000, minimum)
        .unwrap();
    let mut materials = base.resources().materials().to_vec();
    materials[0] = materials[0]
        .with_normal_scale(0)
        .unwrap()
        .with_terrain_contact(contact);
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -4000,
        origin_north_millimeters: -4000,
        sample_spacing_millimeters: 8000,
        heights_millimeters: vec![-3500, 2500, -3500, 2500],
        surface_indices: vec![0; 4],
        surfaces: base.terrain().unwrap().surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: SceneGeometry::new(
            vec![mesh],
            vec![SceneObject::new(
                placement.unwrap_or(*base.geometry().objects()[0].transform()),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: Some(terrain),
        effects: base.effects().cloned(),
    })
    .unwrap()
}

#[test]
fn folded_directions_match_forward_and_both_metadata_passes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (height, weight) in [(0, 0.0), (500, 0.25), (2000, 1.0)] {
        let scene = folded_scene(height);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        let mut camera = pose();
        camera.eye_millimeters[2] += height - 500;
        camera.target_millimeters[2] += height - 500;
        gpu.set_frame(context.queue(), frame_from(camera));
        let sources = normal_sources();
        let expected = glam::Vec3::new(-0.6, 0.0, 0.8).lerp(glam::Vec3::Z, weight);
        let codes = expected
            .to_array()
            .map(|channel| srgb(0.202 + channel * 0.2) * 255.0);
        for code in codes {
            assert!((code.fract() - 0.5).abs() >= 0.1);
        }
        let mut reference = None;
        for source in sources {
            let image = pass_color(&context, &gpu, &layouts, &source, "fs_opaque");
            let center = (32 * TARGET as usize + 32) * 4;
            for (actual, expected) in image[center..center + 3].iter().zip(codes) {
                assert!(
                    (f32::from(*actual) - expected).abs() < 0.5,
                    "height={height}"
                );
            }
            if let Some(reference) = &reference {
                assert_eq!(&image, reference);
            } else {
                reference = Some(image);
            }
        }
    }
}

fn normal_sources() -> [String; 3] {
    let [forward, prepass, metadata, _] = geometry_sources();
    let diagnostic = "vec4<f32>(normal * 0.2 + vec3<f32>(0.202), 1.0)";
    [
        forward.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);", &format!("return {diagnostic};")),
        prepass.replace("return vec4<f32>(normal, 1.0);", &format!("return {diagnostic};")),
        metadata.replace("return vec4<f32>(normal, roughness);", &format!("return {diagnostic};")),
    ]
}

fn mask_settings() -> SceneTranslucency {
    SceneTranslucency::new(SceneTranslucencyParts {
        distortion_micro: 0,
        falloff_micro: 4_000_000,
        sun_scale_micro: 1_000_000,
        ambient_scale_micro: 0,
        scale_micro: 1_000_000,
        base_kelvin: 6500,
        warming_kelvin: 0,
        luminance_micro: 200_000,
        saturation_micro: 0,
    })
    .unwrap()
}

#[test]
fn metalness_mask_is_removed_from_forward_and_reflection_materials() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let base = scene(SceneAlphaMode::Opaque, true, 1);
    let mut materials = base.resources().materials().to_vec();
    materials[0] = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 1_000_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
    .with_translucency_mask(mask_settings());
    assert_eq!(materials[0].translucency_mask(), Some(mask_settings()));
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let [forward, _, metadata, _] = geometry_sources();
    let diagnostic = "vec4<f32>(vec3<f32>(0.202 + metalness * 0.2), 1.0)";
    let forward = forward.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);", &format!("return {diagnostic};"));
    let metadata = metadata.replace(
        "return vec4<f32>(normal, roughness);",
        &format!("return {diagnostic};"),
    );
    let code = srgb(0.202) * 255.0;
    assert!((code.fract() - 0.5).abs() >= 0.1);
    let first = pass_color(&context, &gpu, &layouts, &forward, "fs_opaque");
    assert!((f32::from(first[(32 * TARGET as usize + 32) * 4]) - code).abs() < 0.5);
    assert_eq!(
        first,
        pass_color(&context, &gpu, &layouts, &metadata, "fs_opaque")
    );
}

fn srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

#[test]
fn translated_instances_keep_authored_fold_weights() {
    transformed_fold_weights([1200, 0, 5000], 1_000_000);
}

#[test]
fn scaled_instances_keep_unscaled_authored_fold_weights() {
    transformed_fold_weights([0, 0, -500], 2_000_000);
}

fn transformed_fold_weights(position: [i64; 3], scale: u32) {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (height, weight) in [(0, 0.25), (500, 0.5), (2000, 1.0)] {
        let scene = folded_placement(
            height,
            250_000,
            Some(SceneTransform {
                position_millimeters: position,
                scale_millionths: u64::from(scale),
                ..identity()
            }),
        );
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        let world_z = position[2] + height * i64::from(scale) / 1_000_000;
        let mut camera = pose();
        for point in [&mut camera.eye_millimeters, &mut camera.target_millimeters] {
            point[0] += position[0];
            point[2] += world_z;
        }
        gpu.set_frame(context.queue(), frame_from(camera));
        let expected = glam::Vec3::new(-0.6, 0.0, 0.8).lerp(glam::Vec3::Z, weight);
        let codes = expected.to_array().map(|v| srgb(0.202 + v * 0.2) * 255.0);
        for code in codes {
            assert!((code.fract() - 0.5).abs() >= 0.1);
        }
        for source in normal_sources() {
            let image = pass_color(&context, &gpu, &layouts, &source, "fs_opaque");
            for (actual, expected) in image[(32 * TARGET as usize + 32) * 4..][..3]
                .iter()
                .zip(codes)
            {
                assert!(
                    (f32::from(*actual) - expected).abs() < 0.5,
                    "height={height}, scale={scale}"
                );
            }
        }
    }
}

#[test]
fn mapped_mask_combines_texture_vertex_and_material_ao_once() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for vertex_ao in [300_000, 700_000] {
        let scene = mapped_ao_scene(vertex_ao);
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
        gpu.set_frame(context.queue(), frame_from(pose()));
        let [source, ..] = geometry_sources();
        let source = source.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);",
            "return vec4<f32>(vec3<f32>(0.21 + masked_amount * 0.2), 1.0);");
        let image = pass_color(&context, &gpu, &layouts, &source, "fs_opaque");
        let vertex = if vertex_ao == 300_000 { 0.3f32 } else { 0.7 };
        let packed = vertex.min(132.0 / 255.0);
        let code = srgb(0.21 + 0.8 * packed * packed * 0.6 * 0.2) * 255.0;
        assert!((code.fract() - 0.5).abs() >= 0.1);
        assert!((f32::from(image[(32 * TARGET as usize + 32) * 4]) - code).abs() < 0.5);
    }
}

#[test]
fn rendered_backlight_has_parallel_orthographic_and_diverging_perspective_rays() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let scene = backlight_scene();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &layouts).unwrap();
    let [source, ..] = geometry_sources();
    let source = source.replace("colour += scene_scattering_tint(albedo_sample.rgb, material.scattering_colour,\n            material.scattering_mode.x) * frame.sun_irradiance.rgb * intensity;",
        "colour = vec3<f32>(0.1 + intensity * 0.4);");
    let camera = CameraPose {
        eye_millimeters: [0, -100, 2000],
        target_millimeters: [0, 0, 0],
    };
    for ortho in [true, false] {
        let view = if ortho {
            SceneView::orthographic(
                camera,
                crate::view::OrthographicLens {
                    vertical_half_extent_meters: 1.2,
                    near_meters: 0.1,
                    far_meters: 100.0,
                },
                Viewport {
                    width: TARGET,
                    height: TARGET,
                },
            )
            .unwrap()
        } else {
            SceneView::new(
                camera,
                super::camera(),
                Viewport {
                    width: TARGET,
                    height: TARGET,
                },
            )
            .unwrap()
        };
        let mut frame = FrameUniform::new(&view, scene.effects()).unwrap();
        frame.sun_direction = glam::Vec3::new(0.0, 0.1, -2.0).normalize();
        frame.exposure = 1.0;
        gpu.set_frame(context.queue(), frame);
        let image = pass_color(&context, &gpu, &layouts, &source, "fs_opaque");
        let center = image[(32 * TARGET as usize + 32) * 4];
        let side = image[(32 * TARGET as usize + 12) * 4];
        if ortho {
            assert!(center.abs_diff(side) <= 1, "{center} {side}");
        } else {
            assert!(center > side + 5, "{center} {side}");
        }
    }
}

fn backlight_scene() -> RenderScene {
    let base = scene(SceneAlphaMode::Opaque, true, 1);
    let mut materials = base.resources().materials().to_vec();
    materials[0] = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 1_000_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
    .with_translucency_mask(mask_settings());
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), materials).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn mapped_ao_scene(vertex_ao: u32) -> RenderScene {
    use worldrunner_render_plan::{SceneAmbientOcclusion, SceneMaterialFeatures};
    let mat = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: Some(TextureHandle::from_raw(0)),
        ambient_occlusion: Some(TextureHandle::from_raw(1)),
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 800_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
    .with_features(SceneMaterialFeatures {
        tint: SceneTint::WHITE,
        ambient_occlusion: SceneAmbientOcclusion::new(600_000, true).unwrap(),
        wind: None,
        layer: None,
        translucency: Some(mask_settings()),
    })
    .unwrap();
    let ao = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bc1_with_format(0x8410, 71).into(),
    };
    let lod = quad().lods()[0]
        .clone()
        .with_vertex_occlusion(vec![vertex_ao; 4])
        .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![white_texture(), ao], vec![mat]).unwrap(),
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
