use super::foliage::{geometry_sources, pass_color, pass_depth};
use super::*;
use worldrunner_render_plan::{
    SceneAtlasSampling, SceneImpostor, SceneImpostorChannels, SceneImpostorLayout, TexelFormat,
};

fn atlas() -> SceneImpostor {
    SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap()
}
fn texel(rgba: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: rgba.to_vec().into(),
    }
}
fn scene(atlas: SceneImpostor, textures: Vec<SceneTexture>, packed: bool) -> RenderScene {
    let surface = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: packed.then(|| TextureHandle::from_raw(1)),
        ambient_occlusion: packed.then(|| TextureHandle::from_raw(2)),
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 100_000,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(atlas);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![surface]).unwrap(),
        geometry: SceneGeometry::new(
            vec![card(Some(BillboardPolicy::Spherical))],
            vec![SceneObject::new(
                identity(),
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
fn upload(context: &GpuContext, layouts: &ScenePipelines, scene: &RenderScene) -> SceneGpu {
    let gpu = SceneGpu::upload(context.device(), context.submission(), scene, layouts).unwrap();
    let mut frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 1000],
        target_millimeters: [0, 0, 1000],
    });
    frame.unlit = true;
    gpu.set_frame(context.queue(), frame);
    gpu
}
#[test]
fn per_pass_cutoffs_produce_distinct_shadow_and_surface_silhouettes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let scene = scene(
        atlas().with_pass_cutoffs(970_000, 960_000).unwrap(),
        vec![texel([255, 255, 255, 246])],
        false,
    );
    let gpu = upload(&context, &layouts, &scene);
    let sources = geometry_sources();
    for source in &sources[..3] {
        let pixels = pass_depth(&context, &gpu, &layouts, source, "fs_masked", true);
        assert!(pixels.chunks_exact(4).all(|p| p == 1.0_f32.to_le_bytes()));
    }
    let shadow = pass_depth(
        &context,
        &gpu,
        &layouts,
        &sources[3],
        "fs_depth_masked",
        false,
    );
    assert!(shadow.chunks_exact(4).any(|p| p != 1.0_f32.to_le_bytes()));
}
#[test]
fn implicit_atlas_sampling_uses_minification_mips() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut bytes = Vec::new();
    for level in 0..7 {
        let size = 64_usize >> level;
        let color = if level == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        for _ in 0..size * size {
            bytes.extend_from_slice(&color);
        }
    }
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 64,
        height: 64,
        mips: 7,
        bytes: bytes.into(),
    };
    let source = format!(
        "{}\n@fragment fn fs_sample(input: VertexOutput) -> @location(0) vec4<f32> {{ return scene_atlas_sample(albedo_texture,input); }}",
        crate::shader::ENVIRONMENT_WGSL
    );
    for (sampling, channel) in [
        (SceneAtlasSampling::MipZero, 0),
        (SceneAtlasSampling::Implicit, 1),
    ] {
        let scene = scene(
            atlas().with_sampling(sampling),
            vec![texture.clone()],
            false,
        );
        let gpu = upload(&context, &layouts, &scene);
        let image = pass_color(&context, &gpu, &layouts, &source, "fs_sample");
        assert!(
            image
                .chunks_exact(4)
                .any(|p| p[channel] > 240 && p[1 - channel] < 10)
        );
    }
}
#[test]
fn packed_atlas_surface_and_emission_occlusion_channels_ignore_material_scalars() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let scene = scene(
        atlas().with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion),
        vec![
            texel([40, 40, 40, 255]),
            texel([61, 173, 231, 255]),
            texel([29, 197, 0, 255]),
        ],
        true,
    );
    let gpu = upload(&context, &layouts, &scene);
    let source = format!(
        "{}\n@fragment fn fs_channels(input: VertexOutput) -> @location(0) vec4<f32> {{ let rm=scene_surface_channels(input,scene_atlas_sample(rough_metal_texture,input)); let siao=scene_atlas_first(ambient_occlusion_texture,input); return vec4(rm,siao.g,1.0); }}",
        crate::shader::ENVIRONMENT_WGSL
    );
    let image = pass_color(&context, &gpu, &layouts, &source, "fs_channels");
    let center = usize::try_from((TARGET / 2 * TARGET + TARGET / 2) * 4).unwrap();
    for (actual, linear) in image[center..center + 3]
        .iter()
        .zip([61.0_f64, 173.0, 197.0])
    {
        let expected = (1.055 * (linear / 255.0).powf(1.0 / 2.4) - 0.055) * 255.0;
        assert!((f64::from(*actual) - expected).abs() < 1.0);
    }
    let emitted = pass_color(
        &context,
        &gpu,
        &layouts,
        crate::shader::ENVIRONMENT_WGSL,
        "fs_masked",
    );
    let linear = (40.0_f64 / 255.0) * (1.0 + 29.0 / 255.0);
    let expected = (1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0;
    assert!((expected.fract() - 0.5).abs() >= 0.1);
    assert!((f64::from(emitted[center]) - expected).abs() < 0.5);
}
#[test]
fn translated_capture_center_matches_equivalent_rigid_placement() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let shifted = scene(
        atlas().with_capture_center([500, 0, 0]),
        vec![texel([255; 4])],
        false,
    );
    let base = scene(atlas(), vec![texel([255; 4])], false);
    let mut transform = identity();
    transform.position_millimeters = [500, 0, 0];
    let translated = RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(
            base.geometry().meshes().to_vec(),
            vec![SceneObject::new(
                transform,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let a = upload(&context, &layouts, &shifted);
    let b = upload(&context, &layouts, &translated);
    let source = &geometry_sources()[0];
    assert_eq!(
        pass_color(&context, &a, &layouts, source, "fs_masked"),
        pass_color(&context, &b, &layouts, source, "fs_masked")
    );
}

fn scattering_scene(channel: u8) -> RenderScene {
    use worldrunner_render_plan::{SceneTranslucency, SceneTranslucencyParts};
    let base = scene(
        atlas().with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion),
        vec![
            texel([80, 80, 80, 255]),
            texel([128, channel, 0, 255]),
            texel([0, 128, 0, 255]),
        ],
        true,
    );
    let scattering = SceneTranslucency::new(SceneTranslucencyParts {
        distortion_micro: 0,
        falloff_micro: 1_000_000,
        sun_scale_micro: 1_000_000,
        ambient_scale_micro: 0,
        scale_micro: 1_000_000,
        base_kelvin: 6500,
        warming_kelvin: 0,
        luminance_micro: 500_000,
        saturation_micro: 0,
    })
    .unwrap();
    let material = base.resources().materials()[0].with_translucency_mask(scattering);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
            .unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn mask_oracle(channel: u8, enabled: bool) -> String {
    let source = crate::shader::ENVIRONMENT_WGSL;
    let start = source.find("    var masked_amount = 0.0;").unwrap();
    let end = source[start..]
        .find("    var colour = surface_lighting(")
        .unwrap()
        + start;
    let value = f64::from(channel) / 255.0;
    let (metal, mask) = if value > 0.05 {
        (0.0, value * 128.0 / 255.0)
    } else {
        (value, 0.0)
    };
    let mask = if enabled { mask } else { 0.0 };
    format!(
        "{} let masked_amount: f32 = {mask:?}; metalness = {metal:?};\n{}",
        &source[..start],
        &source[end..]
    )
}

#[test]
fn lit_and_backlit_packed_translucency_pixels_apply_self_ao_once_at_threshold() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    // 12/255 < 0.05 < 13/255, plus a high-mask control with non-unit self AO.
    for channel in [12_u8, 13, 153] {
        let scene = scattering_scene(channel);
        let gpu = upload(&context, &layouts, &scene);
        for direction in [-1.0_f32, 1.0] {
            let mut frame = frame_from(CameraPose {
                eye_millimeters: [0, -4000, 1000],
                target_millimeters: [0, 0, 1000],
            });
            frame.unlit = false;
            frame.sun_direction = glam::Vec3::new(0.0, direction * 0.8, 0.6);
            frame.sun_irradiance = glam::Vec3::splat(0.8);
            frame.ambient_sky = glam::Vec3::splat(0.05);
            frame.ambient_ground = glam::Vec3::splat(0.05);
            gpu.set_frame(context.queue(), frame);
            let actual = pass_color(
                &context,
                &gpu,
                &layouts,
                crate::shader::ENVIRONMENT_WGSL,
                "fs_masked",
            );
            let expected = pass_color(
                &context,
                &gpu,
                &layouts,
                &mask_oracle(channel, true),
                "fs_masked",
            );
            assert!(
                actual
                    .iter()
                    .zip(&expected)
                    .all(|(a, b)| a.abs_diff(*b) <= 1),
                "channel={channel}, direction={direction}"
            );
            if channel == 153 && direction > 0.0 {
                let zero = pass_color(
                    &context,
                    &gpu,
                    &layouts,
                    &mask_oracle(channel, false),
                    "fs_masked",
                );
                let center = usize::try_from((TARGET / 2 * TARGET + TARGET / 2) * 4).unwrap();
                assert!(actual[center] > zero[center] + 10);
            }
        }
    }
}
