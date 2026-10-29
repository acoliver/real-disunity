use super::{capture, capture_gpu, decal, source, srgb, texel};
use crate::draw::{
    SceneGpu, ScenePipelines,
    tests::{COLOR_FORMAT, context, frame_from, pose},
};
use glam::Vec3;
use worldrunner_render_plan::{SceneDecalAlphaMode, SceneDecalColorModifier, SceneResources};

#[test]
fn decal_tint_and_opacity_match_independent_interior_pixel_oracle() {
    let context = context();
    for edge in [false, true] {
        let base = source(false, false);
        for alpha in [100_u8, 128, 134, 150] {
            let mut textures = base.resources().textures().to_vec();
            textures[1] = texel([102, 51, 26, alpha]);
            let scene = base
                .with_resources(
                    SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
                    base.geometry().clone(),
                    None,
                )
                .unwrap();
            let mut d = decal();
            d.dimensions_millimeters = [4000; 3];
            d.edge_fade_micro = 0;
            d.color_modifier.rgb_scale_micro = [1_770_000, 250_000, 500_000];
            d.color_modifier.opacity_micro = 500_000;
            if edge {
                d.color_modifier.alpha_mode = SceneDecalAlphaMode::OpacityEdge;
            }
            let alpha = f32::from(alpha) / 255.0;
            let weight = if edge {
                let t = ((alpha - 0.5) / 0.05).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            } else {
                alpha * 0.5
            };
            let tint = Vec3::new(102.0 * 1.77, 51.0 * 0.25, 26.0 * 0.5);
            let expected = Vec3::new(51.0, 102.0, 153.0).lerp(tint, weight) / 255.0;
            for samples in [1, 4] {
                let pixels = capture(&context, &scene.with_decals(vec![d]).unwrap(), samples);
                for (x, y) in [(30, 30), (32, 32), (34, 34)] {
                    let offset = (y * 64 + x) * 4;
                    for (actual, expected) in
                        pixels[offset..offset + 3].iter().zip(expected.to_array())
                    {
                        assert!((f32::from(*actual) - srgb(expected) * 255.0).abs() < 1.5);
                    }
                }
            }
        }
    }
}

#[test]
fn modifier_only_replace_updates_each_slot_without_rebuilding_palettes() {
    let context = context();
    let d = decal();
    let scene = source(false, false).with_decals(vec![d]).unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let frame = frame_from(pose());
    let mut before = Vec::new();
    for slot in 0..gpu.decals.buffers.len() {
        gpu.select_frame_slot(slot);
        before.push((
            gpu.decals.prepare(&frame, &gpu.frame_buffer),
            gpu.decals.group(),
        ));
    }
    let mut tinted = d;
    tinted.color_modifier.rgb_scale_micro = [1_770_000, 0, 500_000];
    tinted.color_modifier.opacity_micro = 500_000;
    tinted.color_modifier.alpha_mode = SceneDecalAlphaMode::OpacityEdge;
    let buffers = gpu.decals.buffers.clone();
    gpu.set_decal_placements(vec![(0, tinted)]).unwrap();
    let expected = capture(&context, &scene.with_decals(vec![tinted]).unwrap(), 1);
    for (slot, (bytes, group)) in before.iter().enumerate() {
        gpu.select_frame_slot(slot);
        let updated = gpu.decals.prepare(&frame, &gpu.frame_buffer);
        assert_ne!(*bytes, updated);
        assert_eq!(*group, gpu.decals.group());
        assert_eq!(
            expected,
            capture_gpu(&context, &mut gpu, &pipelines, 1, true)
        );
    }
    assert_eq!(buffers, gpu.decals.buffers);
    gpu.set_decal_placements(vec![(0, d)]).unwrap();
    for (slot, (bytes, group)) in before.iter().enumerate() {
        gpu.select_frame_slot(slot);
        assert_eq!(*bytes, gpu.decals.prepare(&frame, &gpu.frame_buffer));
        assert_eq!(*group, gpu.decals.group());
    }
}

fn unmodified_source(source: &str) -> String {
    let call = "let colored = scene_decal_color(sample, decal.color_modifier, (decal.selection.w & 1048576u) != 0u);";
    assert_eq!(source.matches(call).count(), 1);
    source.replace(call, "let colored = sample;")
}

#[test]
fn default_modifier_preserves_rendered_bytes_against_unmodified_shader() {
    let context = context();
    let object = unmodified_source(crate::shader::ENVIRONMENT_WGSL);
    let terrain = unmodified_source(crate::shader::TERRAIN_WGSL);
    for samples in [1, 4] {
        let original = ScenePipelines::with_environment(
            context.device(),
            (COLOR_FORMAT, samples),
            &object,
            &terrain,
            None,
        );
        for terrain_receiver in [false, true] {
            let scene = source(false, terrain_receiver)
                .with_decals(vec![decal()])
                .unwrap();
            let mut gpu =
                SceneGpu::upload(context.device(), context.submission(), &scene, &original)
                    .unwrap();
            let expected = capture_gpu(&context, &mut gpu, &original, samples, true);
            assert_eq!(expected, capture(&context, &scene, samples));
        }
    }
}

#[test]
fn modifier_blends_before_forward_lighting_and_material_metadata_like_baked_texels() {
    let context = context();
    for terrain in [false, true] {
        let scene = source(false, terrain);
        let with_texel = |rgba| {
            let mut textures = scene.resources().textures().to_vec();
            textures[1] = texel(rgba);
            scene
                .with_resources(
                    SceneResources::new(textures, scene.resources().materials().to_vec()).unwrap(),
                    scene.geometry().clone(),
                    Some(crate::draw::tests::effects()),
                )
                .unwrap()
        };
        let mut d = decal();
        d.dimensions_millimeters = [4000; 3];
        d.edge_fade_micro = 0;
        d.color_modifier.rgb_scale_micro = [2_000_000, 500_000, 1_250_000];
        d.color_modifier.opacity_micro = 500_000;
        let actual = with_texel([64, 128, 80, 128]).with_decals(vec![d]).unwrap();
        d.color_modifier = SceneDecalColorModifier::default();
        let expected = with_texel([128, 64, 100, 64]).with_decals(vec![d]).unwrap();
        for samples in [1, 4] {
            assert_eq!(
                capture(&context, &actual, samples),
                capture(&context, &expected, samples)
            );
        }
        let pipelines = material_metadata_pipelines(&context);
        let mut actual_gpu =
            SceneGpu::upload(context.device(), context.submission(), &actual, &pipelines).unwrap();
        let mut expected_gpu = SceneGpu::upload(
            context.device(),
            context.submission(),
            &expected,
            &pipelines,
        )
        .unwrap();
        assert_eq!(
            capture_gpu(&context, &mut actual_gpu, &pipelines, 1, true),
            capture_gpu(&context, &mut expected_gpu, &pipelines, 1, true)
        );
    }
}

#[test]
fn opacity_edge_thresholds_angle_faded_alpha_for_mesh_and_terrain() {
    let context = context();
    let metadata = channel_pipelines(&context);
    for terrain in [false, true] {
        let base = partial_angle_source(terrain);
        for opacity in [500_000, 625_000, 700_000] {
            let mut d = decal();
            d.dimensions_millimeters = [4000; 3];
            d.edge_fade_micro = 0;
            d.angle_cutoff_micro = 788_854;
            d.angle_fade_micro = 211_146;
            d.color_modifier.alpha_mode = SceneDecalAlphaMode::OpacityEdge;
            d.color_modifier.opacity_micro = opacity;
            let expected = partial_angle_oracle(terrain, opacity);
            let scene = base.with_decals(vec![d]).unwrap();
            for samples in [1, 4] {
                assert_interior(&capture(&context, &scene, samples), expected[0]);
            }
            for (pipelines, expected) in metadata.iter().zip(expected) {
                let mut gpu =
                    SceneGpu::upload(context.device(), context.submission(), &scene, pipelines)
                        .unwrap();
                assert_interior(
                    &capture_gpu(&context, &mut gpu, pipelines, 1, true),
                    expected,
                );
            }
        }
    }
}

fn partial_angle_source(terrain: bool) -> worldrunner_render_plan::RenderScene {
    use worldrunner_render_plan::{
        SceneAlphaMode, SceneMaterial, SceneMaterialParts, TextureHandle,
    };
    let base = source(true, terrain);
    let mut textures = base.resources().textures().to_vec();
    textures[1] = texel([204, 51, 26, 204]);
    textures.push(worldrunner_render_plan::SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rg8Unorm,
        color_space: worldrunner_render_plan::TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128, 191].into(),
    });
    textures.push(texel([51, 249, 204, 255]));
    let mut materials = base.resources().materials().to_vec();
    materials[1] = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: Some(TextureHandle::from_raw(4)),
        roughness_metallic: Some(TextureHandle::from_raw(5)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 900_000,
        metalness_micro: 100_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
    .with_alpha_factor(600_000)
    .unwrap();
    base.with_resources(
        SceneResources::new(textures, materials).unwrap(),
        base.geometry().clone(),
        None,
    )
    .unwrap()
}

fn partial_angle_oracle(terrain: bool, opacity: u32) -> [Vec3; 3] {
    let geometric = Vec3::new(-0.5, 0.0, 1.0).normalize();
    let angle = (geometric.z - 0.788_854) / 0.211_146;
    assert!((angle - 0.5).abs() < 0.000_01);
    let lower = 1.0 - f32::from(u16::try_from(opacity / 1000).unwrap()) / 1000.0;
    let t = ((0.8 * angle - lower) / 0.05).clamp(0.0, 1.0);
    let weight = t * t * (3.0 - 2.0 * t) * 0.6;
    if opacity == 500_000 {
        assert_eq!(weight.to_bits(), 0.0_f32.to_bits());
    }
    let albedo = Vec3::new(51.0, 102.0, 153.0).lerp(Vec3::new(204.0, 51.0, 26.0), weight) / 255.0;
    let x = 1.0 / 255.0;
    let y = 127.0 / 255.0;
    let detail = Vec3::new(y, -x, (1.0_f32 - x * x - y * y).sqrt());
    let corrected = glam::Quat::from_rotation_arc(Vec3::Z, geometric) * detail;
    let receiver = if terrain {
        let tangent = Vec3::Z.cross(geometric).normalize();
        let bitangent = geometric.cross(tangent);
        (tangent * x + bitangent * x + geometric * (1.0_f32 - 2.0 * x * x).sqrt()).normalize()
    } else {
        geometric
    };
    let normal = receiver.lerp(corrected, weight).normalize() * 0.5 + Vec3::splat(0.5);
    let properties = Vec3::new(1.0, 0.0, 0.0).lerp(Vec3::new(0.2, 0.8, 0.0), weight);
    [albedo, normal, properties]
}

fn assert_interior(pixels: &[u8], expected: Vec3) {
    for (x, y) in [(30, 30), (32, 32), (34, 34)] {
        let offset = (y * 64 + x) * 4;
        for (actual, linear) in pixels[offset..offset + 3].iter().zip(expected.to_array()) {
            assert!(
                (f32::from(*actual) - srgb(linear) * 255.0).abs() < 2.0,
                "pixel={x},{y} actual={actual} expected={linear}"
            );
        }
    }
}

fn material_metadata_pipelines(context: &crate::device::GpuContext) -> ScenePipelines {
    metadata_pipelines(
        context,
        "vec4<f32>(mix(vec3<f32>(0.04), albedo.rgb, metalness), roughness)",
        "vec4<f32>(mix(vec3<f32>(0.04), projected.albedo, projected.properties.y), projected.properties.x)",
    )
}

fn metadata_pipelines(
    context: &crate::device::GpuContext,
    object_output: &str,
    terrain_output: &str,
) -> ScenePipelines {
    let object = crate::shader::ENVIRONMENT_WGSL
        .replace("fn fs_relief_opaque(", "fn unused_relief_opaque(")
        .replace("fn fs_relief_masked(", "fn unused_relief_masked(")
        .replace("fn fs_opaque(", "fn unused_opaque(")
        .replace("fn fs_masked(", "fn unused_masked(");
    let object = format!(
        "{object}\n{}",
        include_str!("../../scene_metadata/geometry.wgsl")
            .replace("METADATA_OUTPUT", object_output)
    );
    let terrain = crate::shader::TERRAIN_WGSL.replace(
        "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), 1.0);",
        &format!("return {terrain_output};"),
    );
    ScenePipelines::with_environment(context.device(), (COLOR_FORMAT, 1), &object, &terrain, None)
}

fn channel_pipelines(context: &crate::device::GpuContext) -> [ScenePipelines; 3] {
    [
        metadata_pipelines(
            context,
            "vec4<f32>(albedo.rgb, 1.0)",
            "vec4<f32>(projected.albedo, 1.0)",
        ),
        metadata_pipelines(
            context,
            "vec4<f32>(normal * 0.5 + 0.5, 1.0)",
            "vec4<f32>(projected.normal * 0.5 + 0.5, 1.0)",
        ),
        metadata_pipelines(
            context,
            "vec4<f32>(roughness, metalness, 0.0, 1.0)",
            "vec4<f32>(projected.properties, 0.0, 1.0)",
        ),
    ]
}

#[test]
fn opacity_edge_with_disabled_albedo_and_alpha_test_matches_scalar_oracle() {
    use worldrunner_render_plan::{SceneDecalNormalFrame, SceneDecalWriteMask};
    let context = context();
    let pipelines = channel_pipelines(&context);
    for terrain in [false, true] {
        let base = partial_angle_source(terrain);
        let receiver = partial_angle_oracle(terrain, 500_000);
        for mask in [4, 6] {
            for (cutoff, accepted) in [(350_000, true), (450_000, false)] {
                let mut d = decal();
                d.dimensions_millimeters = [4000; 3];
                d.edge_fade_micro = 0;
                d.angle_cutoff_micro = 788_854;
                d.angle_fade_micro = 211_146;
                d.mapping.normal_frame = SceneDecalNormalFrame::ReceiverWorld;
                d.color_modifier = SceneDecalColorModifier {
                    rgb_scale_micro: [0, 4_000_000, 2_000_000],
                    opacity_micro: 625_000,
                    alpha_mode: SceneDecalAlphaMode::OpacityEdge,
                };
                d.write_mask = SceneDecalWriteMask::from_bits(mask);
                d.alpha_test_cutoff_micro = Some(cutoff);
                let mut expected = if accepted {
                    partial_angle_oracle(terrain, 625_000)
                } else {
                    receiver
                };
                expected[0] = receiver[0];
                if mask & 2 == 0 {
                    expected[1] = receiver[1];
                }
                let scene = base.with_decals(vec![d]).unwrap();
                for (pipeline, expected) in pipelines.iter().zip(expected) {
                    let mut gpu =
                        SceneGpu::upload(context.device(), context.submission(), &scene, pipeline)
                            .unwrap();
                    assert_interior(
                        &capture_gpu(&context, &mut gpu, pipeline, 1, true),
                        expected,
                    );
                }
            }
        }
    }
}
