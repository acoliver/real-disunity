use super::*;
use glam::Vec3;
use worldrunner_render_plan::{
    SceneRoughnessRemap, SceneSpecularConvention, SceneSpecularVisibility, TexelFormat,
};

fn selected() -> SceneSpecularConvention {
    SceneSpecularConvention {
        roughness: SceneRoughnessRemap::Perceptual045,
        visibility: SceneSpecularVisibility::SchlickSmith,
        ..Default::default()
    }
}
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
fn selected_scene(kind: u8) -> RenderScene {
    let mesh = selected_mesh(kind);
    let mut parts = effect_parts();
    parts.ambient.sky_irradiance_micro = 0;
    parts.ambient.ground_irradiance_micro = 0;
    let mat = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: (kind == 1).then(|| TextureHandle::from_raw(1)),
        roughness_metallic: (kind == 1).then(|| TextureHandle::from_raw(2)),
        roughness_micro: 500_000,
        metalness_micro: 0,
        ambient_occlusion: None,
        self_illumination: None,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                texel([255; 4]),
                texel([128, 128, 255, 255]),
                texel([0, 128, 0, 255]),
            ],
            vec![mat],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![mesh],
            if kind == 1 {
                vec![]
            } else {
                vec![SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                )]
            },
        )
        .unwrap(),
        terrain: (kind == 1).then(|| terrain(1)),
        effects: Some(
            SceneEffects::new(parts)
                .unwrap()
                .with_ssr(request())
                .unwrap()
                .with_specular_convention(selected()),
        ),
    })
    .unwrap()
}

#[test]
fn selected_mesh_terrain_and_card_draws_publish_environment_separately_from_direct() {
    let context = context();
    let maps = environment_convention(&context, true, selected());
    for kind in 0..3 {
        let source = selected_scene(kind);
        let mut frame = texel_center_frame(&source);
        frame.sun_direction = Vec3::new(0.2, -0.3, 1.0).normalize();
        frame.sun_irradiance = Vec3::splat(0.5);
        let lit = render_with_environment(&context, &source, 1, &maps, &frame);
        frame.sun_irradiance = Vec3::ZERO;
        let ambient = render_with_environment(&context, &source, 1, &maps, &frame);
        let env = channel(&context, lit.ssr.as_ref().unwrap().textures()[2], 0, 0);
        let factor = channel(&context, lit.ssr.as_ref().unwrap().textures()[3], 0, 0);
        let direct_color = channel(&context, lit.targets().scene().texture(), 0, 0);
        let ambient_color = channel(&context, ambient.targets().scene().texture(), 0, 0);
        let mut checked = 0;
        for i in 0..32 * 32 {
            if factor[i] > 0.0 {
                assert!(env[i] > 0.0);
                assert!(
                    (ambient_color[i] - env[i]).abs() < 0.002,
                    "pass {kind}, pixel {i}"
                );
                assert!(direct_color[i] > ambient_color[i] + 0.05);
                checked += 1;
            }
        }
        assert!(checked > 16, "pass {kind}");
    }
}

fn selected_water(missing: bool) -> RenderScene {
    let base = source(true, missing);
    let mut parts = effect_parts();
    parts.sun.irradiance_micro = 0;
    parts.ambient.sky_irradiance_micro = 0;
    parts.ambient.ground_irradiance_micro = 0;
    parts.water = Some(SceneWaterBody {
        roughness_micro: 10_000,
        material: SceneWaterSettings {
            normal: Some(TextureHandle::from_raw(2)),
            ..SceneWaterSettings::STOCK
        },
        ..body()
    });
    let mat = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: Some(TextureHandle::from_raw(1)),
        roughness_micro: 500_000,
        metalness_micro: 1_000_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![
                texel([0, 0, 0, 255]),
                texel([255; 4]),
                texel([128, 128, 255, 255]),
            ],
            vec![mat],
        )
        .unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(
            SceneEffects::new(parts)
                .unwrap()
                .with_ssr(request())
                .unwrap()
                .with_specular_convention(selected()),
        ),
    })
    .unwrap()
}

#[test]
fn selected_subfloor_water_environment_and_ssr_preserve_direct_specular() {
    let context = context();
    let maps = environment_convention(&context, true, selected());
    let mut deltas = Vec::new();
    for missing in [false, true] {
        let source = selected_water(missing);
        let mut frame = frame(&source);
        frame.sun_direction = Vec3::new(0.0, 0.6, 0.8);
        frame.sun_irradiance = Vec3::splat(32.0);
        let lit = render_with_environment(&context, &source, 1, &maps, &frame);
        frame.sun_irradiance = Vec3::ZERO;
        let dark = render_with_environment(&context, &source, 1, &maps, &frame);
        let (normal, material, env) = lit.water_receivers().unwrap();
        let coverage = channel(&context, normal.texture(), 0, 3);
        let coordinate = channel(&context, material.texture(), 0, 3);
        let environment = channel(&context, env.texture(), 0, 0);
        let lit_color = channel(&context, lit.targets().scene().texture(), 0, 0);
        let dark_color = channel(&context, dark.targets().scene().texture(), 0, 0);
        let expected = environment_reference(&context, &maps, &lit, &frame, false);
        let mut delta = vec![0.0; 32 * 32];
        let mut checked = 0;
        for y in 17..29 {
            for x in 13..19 {
                let i = y * 32 + x;
                if coverage[i] == 0.0 {
                    continue;
                }
                assert!((coordinate[i] - 0.045).abs() < 0.00005);
                assert!(environment[i] > 0.0);
                assert!(
                    (environment[i] - expected[i]).abs() < 0.001,
                    "water forward/factor mismatch at {i}: {} != {}",
                    environment[i],
                    expected[i]
                );
                delta[i] = lit_color[i] - dark_color[i];
                assert!(
                    delta[i] > 0.0001,
                    "water direct specular at {i}: {}",
                    delta[i]
                );
                checked += 1;
            }
        }
        assert!(checked > 16);
        if !missing {
            let confidence = channel(&context, lit.ssr.as_ref().unwrap().textures()[4], 0, 3);
            assert!(
                confidence.iter().any(|v| *v > 0.1),
                "must exercise replacement hits"
            );
        }
        assert_replacement_factor(&context, &maps, &lit, &frame, &environment);
        deltas.push(delta);
    }
    for (hit, miss) in deltas[0].iter().zip(&deltas[1]) {
        assert!(
            (hit - miss).abs() < 0.002,
            "replacement removed direct specular: {hit} != {miss}"
        );
    }
}

fn matrix_literal(matrix: glam::Mat4) -> String {
    let values = matrix
        .to_cols_array()
        .iter()
        .map(|v| format!("{v:?}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("mat4x4<f32>({values})")
}
fn environment_reference(
    context: &GpuContext,
    maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    renderer: &FrameRenderer,
    frame: &FrameUniform,
    factor_only: bool,
) -> Vec<f32> {
    let (normals, material, _) = renderer.water_receivers().unwrap();
    let (cube, lut) = maps.views().unwrap();
    let shader = format!(
        include_str!("specular_reference.wgsl"),
        matrix_literal(frame.view.inverse()),
        matrix_literal(frame.view_projection.inverse()),
        frame.camera_position.x,
        frame.camera_position.y,
        frame.camera_position.z
    );
    let shader = shader.replace("return radiance*factor;", reference_return(factor_only));
    let pipeline = crate::ssao::passes::pipeline(context.device(), &shader);
    let sampler = reference_sampler(context.device());
    let depth = renderer
        .targets()
        .depth()
        .texture()
        .create_view(&TextureViewDescriptor::default());
    let views = [&normals, &material, &depth, &cube, &lut];
    let mut entries: Vec<_> = views
        .iter()
        .enumerate()
        .map(|(i, view)| wgpu::BindGroupEntry {
            binding: u32::try_from(i).unwrap(),
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    entries.push(wgpu::BindGroupEntry {
        binding: 5,
        resource: wgpu::BindingResource::Sampler(&sampler),
    });
    let group = context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
    reference_draw(context, &pipeline, &group)
}
fn reference_draw(
    context: &GpuContext,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
) -> Vec<f32> {
    let target = crate::ssao::passes::texture(context.device(), [32, 32], TextureFormat::R32Float);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        RenderPassColorAttachment {
            view: &target.create_view(&TextureViewDescriptor::default()),
            resolve_target: None,
            depth_slice: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        },
        pipeline,
        &[group],
    );
    encoder.finish().submit().unwrap();
    Readback::new(
        context.device(),
        FrameConfig::new([32, 32], TextureFormat::R32Float, 1),
        100_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), &target)
    .unwrap()
    .chunks_exact(4)
    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
    .collect()
}

fn selected_mesh(kind: u8) -> SceneMesh {
    let mut mesh = quad();
    if kind == 2 {
        mesh = SceneMesh::new(vec![
            SceneMeshLod::new(
                0,
                quad().lods()[0].vertices().to_vec(),
                vec![0, 1, 2, 0, 2, 3],
                Some(BillboardPolicy::YawLocked),
            )
            .unwrap(),
        ])
        .unwrap();
    }
    mesh
}

fn reference_sampler(device: &Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

fn assert_replacement_factor(
    context: &GpuContext,
    maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    renderer: &FrameRenderer,
    frame: &FrameUniform,
    environment: &[f32],
) {
    let expected = environment_reference(context, maps, renderer, frame, true);
    let replay = crate::ssr::tests::water_tests::replay_water_factor(context, renderer, frame);
    for y in 17..29 {
        for x in 13..19 {
            let i = y * 32 + x;
            let value = positive_half(&replay[i * 8..i * 8 + 2]);
            let actual = value - 1.0 + environment[i];
            assert!(
                (actual - expected[i]).abs() < 0.001,
                "SSR factor at {i}: {actual} != {}",
                expected[i]
            );
        }
    }
}

fn positive_half(bytes: &[u8]) -> f32 {
    let bits = u16::from_le_bytes(bytes.try_into().unwrap());
    let exponent = i32::from((bits >> 10) & 31);
    let fraction = f32::from(bits & 1023) / 1024.0;
    if exponent == 0 {
        fraction * 2.0f32.powi(-14)
    } else {
        (1.0 + fraction) * 2.0f32.powi(exponent - 15)
    }
}

fn reference_return(factor_only: bool) -> &'static str {
    if factor_only {
        "return factor;"
    } else {
        "return radiance*factor;"
    }
}
