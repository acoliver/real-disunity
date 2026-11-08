//! Frozen metadata evaluation before alpha-first rejection. Synthetic GPU oracle only.
use super::*;

#[path = "metadata_forward_sources.rs"]
mod final_forward_sources;

const GEOMETRY: &str = r"fn reflection_metadata(original: VertexOutput, front: bool, masked: bool, position: vec3<f32>) -> METADATA_TYPE {
    var input = original;
    input.world_position = position;
    let footprint = scene_atlas_footprint(albedo_texture, input);
    var albedo = scene_albedo(scene_atlas_sample(albedo_texture, input, footprint), input.tint);
    let surface = scene_surface_channels(input);
    let geometric = normalize(input.world_normal) * select(-1.0, 1.0, front);
    let mapped_normal = sample_normal(input, geometric, input.world_position, input.world_tangent);
    var normal = scene_fold_direction(mapped_normal, input.ground_fold, material.contact_surface.zw);
    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        roughness = surface.x;
        metalness = surface.y;
    }
    let contact_weight = scene_contact(input.world_position);
    if contact_weight != 0.0 {
        albedo = vec4<f32>(scene_contact_albedo(albedo.rgb, contact_weight), albedo.a);
        let surface = scene_contact_surface(vec2<f32>(roughness, metalness), contact_weight);
        roughness = surface.x;
        metalness = surface.y;
    }
    if material.scattering_mode.y != 0.0 && (!scene_packed_atlas(input) || metalness > 0.05) { metalness = 0.0; }
    if scene_packed_atlas(input) && metalness > 0.05 { metalness = 0.0; }
    let projected = scene_project_decals(input.world_position, geometric, 1u,
        albedo.rgb, vec2<f32>(roughness, metalness), normal);
    normal = projected.normal;
    albedo = vec4<f32>(projected.albedo, albedo.a);
    roughness = projected.properties.x;
    metalness = projected.properties.y;
    scene_apply_coverage(input);
    if masked && !scene_alpha_covered(albedo.a, input) { discard; }
    return METADATA_OUTPUT;
}
fn scene_metadata_direction(direction: vec3<f32>) -> vec3<f32> {
    if material.contact_surface.z > 0.0 { return direction; }
    return normalize(direction);
}
";

const NORMAL: &str = r"// Decode source channels before scaling tangent-space XY.
fn sample_normal(input: VertexOutput, geometric: vec3<f32>, position: vec3<f32>, tangent: vec4<f32>) -> vec3<f32> {
    if (material.texture_flags & FLAG_NORMAL) == 0u {
        return geometric;
    }
    let footprint = scene_atlas_footprint(normal_texture, input);
    var frame_basis = cotangent_frame(geometric, position, input.texcoord);
    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if input.atlas_eye.w != 0.0 && (material.impostor_relief.x != 0.0 || scene_packed_atlas(input)) { return scene_relief_normal(input, footprint); }
        }
    }
    let sampled = scene_atlas_sample(normal_texture, input, footprint).rgb * 2.0 - 1.0;
    var xy = sampled.xy;
    var z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
    if (material.geometry_flags.w & 1u) != 0u { z = sampled.z; }
    if (material.geometry_flags.w & 2u) != 0u { xy.y = -xy.y; }
    let scale = bitcast<f32>(material.geometry_flags.z);
    if scale == 0.0 { return geometric; }
    xy *= scale;
    if tangent.w != 0.0 {
        let t = normalize(tangent.xyz - geometric * dot(geometric, tangent.xyz));
        frame_basis = mat3x3<f32>(t, cross(geometric, t) * tangent.w, geometric);
    }
    return normalize(frame_basis * vec3<f32>(xy, z));
}
";

const DECALS: &str = r"fn scene_project_decals(position: vec3<f32>, geometric: vec3<f32>, receiver: u32,
    albedo: vec3<f32>, properties: vec2<f32>, normal: vec3<f32>) -> SceneDecalSurface {
    var result = SceneDecalSurface(albedo, properties, normal);
    let world_dx = dpdx(position);
    let world_dy = dpdy(position);
    let gradient_scale = exp2(frame.lighting_options.w);
    var candidates = scene_decal_candidates(position);
    while candidates != 0u {
        let i = firstTrailingBit(candidates);
        candidates &= candidates - 1u;
        let decal = scene_decals.volumes[i];
        if (decal.selection.x & receiver) == 0u { continue; }
        let local = (decal.local_from_world * vec4<f32>(position, 1.0)).xyz;
        let edge = 0.5 - max(max(abs(local.x), abs(local.y)), abs(local.z));
        if edge < 0.0 { continue; }
        let angle_fade = scene_decal_angle(dot(geometric, decal.direction_edge.xyz), decal.surface_angle.zw, decal.reserved.yzw);
        var weight = 1.0;
        if angle_fade == 0.0 { continue; }
        if decal.direction_edge.w > 0.0 { weight *= clamp(edge / decal.direction_edge.w, 0.0, 1.0); }
        let uv = (scene_decal_uv(local, decal.selection.z) + 0.5) * decal.uv_transform.xy + decal.uv_transform.zw;
        let dx = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dx, 0.0)).xyz, decal.selection.z) * decal.uv_transform.xy * gradient_scale;
        let dy = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dy, 0.0)).xyz, decal.selection.z) * decal.uv_transform.xy * gradient_scale;
        var sample = textureSampleGrad(scene_decal_albedo, scene_decal_sampler, uv, i32(decal.selection.y), dx, dy);
        sample.a *= angle_fade;
        if (decal.selection.x & 4194304u) != 0u {
            let cutoff = f32((decal.selection.x >> 2u) & 1048575u) / 1000000.0;
            if sample.a < cutoff { continue; }
        }
        let colored = scene_decal_color(sample, decal.color_modifier, (decal.selection.w & 1048576u) != 0u);
        weight *= colored.a * decal.reserved.x;
        if (decal.selection.w & 65536u) == 0u {
            result.albedo = mix(result.albedo, colored.rgb, weight);
        }
        var surface = decal.surface_angle.xy;
        let surface_layer = (decal.selection.w >> 8u) & 255u;
        if surface_layer != 0u {
            surface = textureSampleGrad(scene_decal_surface, scene_decal_sampler, uv, i32(surface_layer - 1u), dx, dy).rb;
        }
        if (decal.selection.w & 262144u) == 0u {
            result.properties = mix(result.properties, surface, weight);
        }
        let normal_layer = decal.selection.w & 255u;
        if normal_layer != 0u && weight > 0.0 && (decal.selection.w & 131072u) == 0u {
            let encoded = textureSampleGrad(scene_decal_normal, scene_decal_sampler, uv, i32(normal_layer - 1u), dx, dy).rg;
            let detail = scene_decal_correct_normal(decal, geometric, encoded);
            result.normal = scene_decal_blend_normal(result.normal, detail, weight);
        }
    }
    return result;
}
";

const SURFACE: &str = r"fn scene_surface_channels(input: VertexOutput) -> vec2<f32> {
    if (material.texture_flags & FLAG_ROUGH_METAL) == 0u { return vec2<f32>(material.roughness, material.metalness); }
    let footprint = scene_atlas_footprint(rough_metal_texture, input);
    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if scene_packed_atlas(input) { return scene_atlas_first(rough_metal_texture, footprint).rg; }
        }
    }
    let sampled = scene_atlas_sample(rough_metal_texture, input, footprint);
    return vec2<f32>(material.roughness * sampled.g, material.metalness * sampled.b);
}
";

const SINGLE: &str = r"@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    let depth = scene_relief_depth(input);
    let metadata = reflection_metadata(input, front, false, depth.position);
    if !depth.visible { discard; }
    return SceneSurfaceOutput(metadata, depth.value);
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    let depth = scene_relief_depth(input);
    let metadata = reflection_metadata(input, front, true, depth.position);
    if !depth.visible { discard; }
    return SceneSurfaceOutput(metadata, depth.value);
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,false,scene_relief_position(input)); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,true,scene_relief_position(input)); }
";

const MERGED: &str = r"struct SceneMetadataOutput { @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, }
struct SceneMetadataReliefOutput { @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, @builtin(frag_depth) depth: f32, }

@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataReliefOutput {
    let depth = scene_relief_depth(input);
    let surface = reflection_metadata(input, front, false, depth.position);
    if !depth.visible { discard; }
    return SceneMetadataReliefOutput(surface.normal, surface.material, depth.value);
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataReliefOutput {
    let depth = scene_relief_depth(input);
    let surface = reflection_metadata(input, front, true, depth.position);
    if !depth.visible { discard; }
    return SceneMetadataReliefOutput(surface.normal, surface.material, depth.value);
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataOutput { return reflection_metadata(input,front,false,scene_relief_position(input)); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataOutput { return reflection_metadata(input,front,true,scene_relief_position(input)); }
";

pub(super) fn pipelines(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    output: MetadataOutput,
) -> (ScenePipelines, wgpu::BindGroup) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("reference metadata camera"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let mut base = crate::shader::ENVIRONMENT_WGSL.to_owned();
    for entry in [
        "fs_opaque",
        "fs_masked",
        "fs_relief_opaque",
        "fs_relief_masked",
    ] {
        base = base.replace(&format!("fn {entry}("), &format!("fn unused_{entry}("));
    }
    let normal = "vec4<f32>(scene_metadata_direction((screen.view * vec4<f32>(projected.normal, 0.0)).xyz), 1.0)";
    let material = "vec4<f32>(scene_material_f0(albedo.rgb, metalness, DIELECTRIC_F0), roughness)";
    let (kind, value, entries) = match output {
        MetadataOutput::Normal => ("vec4<f32>", normal.to_owned(), SINGLE),
        MetadataOutput::Merged => (
            "SceneMetadataOutput",
            format!("SceneMetadataOutput({normal}, {material})"),
            MERGED,
        ),
        MetadataOutput::Material => {
            panic!("reference fixture compares normal and merged pipelines")
        }
    };
    let mut frozen = format!("{NORMAL}\n{DECALS}\n{SURFACE}\n{GEOMETRY}\n{entries}")
        .replace("METADATA_TYPE", kind)
        .replace("METADATA_OUTPUT", &value);
    for function in [
        "sample_normal",
        "scene_project_decals",
        "scene_surface_channels",
    ] {
        frozen = frozen.replace(&format!("{function}("), &format!("reference_{function}("));
    }
    let camera = include_str!("../../ssao/camera.wgsl");
    let object = format!("{base}\n{camera}\n{frozen}");
    let terrain = format!(
        "{}\n{camera}",
        output.terrain_source(crate::shader::TERRAIN_WGSL)
    );
    (
        ScenePipelines::with_screen_space(
            device,
            (wgpu::TextureFormat::Rgba16Float, 1),
            &object,
            &terrain,
            None,
            Some(&layout),
        ),
        group,
    )
}

#[test]
fn forward_helpers_preserve_pre_change_source_exactly() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (shadowed, terrain) = crate::shadows::capture_sources(true);
    for source in [crate::shader::ENVIRONMENT_WGSL, shadowed] {
        for helper in [NORMAL, DECALS] {
            assert!(source.contains(helper), "forward helper source changed");
        }
    }
    let selected_surface = SURFACE
        .replace(
            "if scene_packed_atlas(input) { return scene_atlas_first(rough_metal_texture, footprint).rg; }",
            "if scene_packed_atlas(input) {\n                let sampled = scene_atlas_first(rough_metal_texture, footprint);\n                return vec2<f32>(sampled[material.packed_atlas_channels[0].x + 0], sampled[material.packed_atlas_channels[1].x + 1]);\n            }",
        )
        .replace("sampled.g", "sampled[material.scalar_channels[0].x + 1]")
        .replace("sampled.b", "sampled[material.scalar_channels[1].x + 2]");
    for source in [crate::shader::ENVIRONMENT_WGSL, shadowed] {
        assert!(
            source.contains(&selected_surface),
            "forward surface source changed"
        );
    }
    for source in [crate::shader::TERRAIN_WGSL, terrain] {
        assert!(source.contains(DECALS), "terrain decal source changed");
    }
}

// Freeze production assemblies with invariant paired positions. Unpaired hashes stay unchanged.
// Hash every byte, including unreachable declarations, to detect changes shared by both GPU runs.
fn source_fingerprint(source: &str) -> (usize, u128) {
    let hash = source.bytes().fold(
        0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128,
        |hash, byte| (hash ^ u128::from(byte)).wrapping_mul(0x0100_0000_0000_0000_0000_013b),
    );
    (source.len(), hash)
}

#[test]
fn forward_assemblies_match_invariant_baseline_fingerprints() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (shadowed, shadowed_terrain) = crate::shadows::capture_sources(true);
    for (source, expected) in [
        (
            crate::shader::ENVIRONMENT_WGSL,
            (93_482, 0x22e8_31dc_934f_cb24_2e4f_660c_23cb_6188),
        ),
        (
            crate::shader::TERRAIN_WGSL,
            (48_980, 0xa055_4b91_69fa_ff16_c593_ee7c_7301_a789),
        ),
        (
            shadowed,
            (109_371, 0xb4d0_a58f_16bb_82c2_bfcd_6115_674f_bc2b),
        ),
        (
            shadowed_terrain,
            (64_869, 0xdb31_8164_dd59_1240_4592_cc2e_6f1a_adba),
        ),
    ] {
        assert_eq!(source_fingerprint(source), expected);
    }
    let lighting =
        worldrunner_render_plan::SceneCelestialLighting::new(500_000, 2_000_000, 3_000_000)
            .unwrap();
    for (source, expected) in [
        (
            crate::shader::ENVIRONMENT_WGSL,
            (93_717, 0xb135_4496_3393_36fb_606f_47d2_2966_2510),
        ),
        (
            shadowed,
            (109_606, 0x11db_84ef_7b27_fd05_043f_3ff9_dadb_b5c9),
        ),
    ] {
        assert_eq!(
            source_fingerprint(&crate::sky::lighting::source(source.to_owned(), lighting)),
            expected
        );
    }
}

const CELESTIAL_SOURCES: [[(usize, u128); 4]; 4] = [
    [
        (95_070, 0x79d1_c93f_0185_4adc_0b5e_27f6_b394_08e9),
        (95_044, 0xc9c0_7167_b659_8ef4_d514_7d77_f78b_ffe3),
        (95_305, 0x2f8a_85c1_eaf9_2293_8e51_4a76_d7d7_a263),
        (95_279, 0x7013_328b_8375_8863_2a86_00fb_b361_5ec1),
    ],
    [
        (94_958, 0x8a66_275b_9492_7436_aa52_7773_20ab_7ab5),
        (94_932, 0xbb15_b43b_2d87_779d_d08d_afde_c093_8caf),
        (95_193, 0x5c9a_6492_f218_715d_7576_0657_1e57_47a5),
        (95_167, 0xa199_1340_5fe2_259a_ca38_2659_ea87_fc7f),
    ],
    [
        (95_096, 0x1f85_097d_edbf_4db1_65ae_129a_77d2_6159),
        (95_070, 0x0f27_948f_15cc_ed38_619a_bfd8_6207_1773),
        (95_331, 0xf867_2a82_2d54_8721_79c6_18fa_d402_17cf),
        (95_305, 0x6394_ef8a_92cb_41f8_319c_e53d_0c11_79fd),
    ],
    [
        (95_038, 0xfd74_b3fe_26f3_7ad6_51cc_c728_0edb_7199),
        (95_012, 0xf3ed_c0e4_d269_050d_88ac_7c6f_c218_fe33),
        (95_273, 0x8834_c8b8_009f_24e4_fce0_d0ce_051b_27bd),
        (95_247, 0xa767_4fb2_903a_029b_19f7_2fb3_0ff5_09b7),
    ],
];

#[test]
fn celestial_forward_assemblies_preserve_complete_pre_change_source() {
    use worldrunner_render_plan::SceneCelestialLighting;
    let _session = crate::test_support::SceneTestSession::acquire();
    for (composition, index) in [
        ((false, false, false), 0),
        ((false, false, true), 3),
        ((false, true, false), 2),
        ((false, true, true), 2),
        ((true, false, false), 1),
        ((true, false, true), 1),
        ((true, true, false), 2),
        ((true, true, true), 2),
    ] {
        for (selected, lighting) in [
            SceneCelestialLighting::NEUTRAL,
            SceneCelestialLighting::new(500_000, 2_000_000, 3_000_000).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            for anchored in [false, true] {
                let source = crate::renderer::celestial_source(composition, lighting, anchored);
                assert_eq!(
                    source_fingerprint(&source),
                    CELESTIAL_SOURCES[index][selected * 2 + usize::from(anchored)]
                );
            }
        }
    }
}

#[test]
fn forward_fingerprint_detects_even_unused_source_byte_mutations() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let source = crate::shader::ENVIRONMENT_WGSL;
    let changed = format!("{source}\n// unused declaration mutation\n");
    assert_ne!(source_fingerprint(source), source_fingerprint(&changed));
}

#[test]
fn directional_shadow_assembly_preserves_frozen_non_invariant_source() {
    assert_eq!(
        source_fingerprint(&crate::shadows::depth_source()),
        (93_836, 0x7829_6e87_95d4_17d7_4010_afbe_550c_ba3c),
    );
}
