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
            (95_438, 0x426c_8723_c9fb_4291_8336_e4d6_8445_36aa),
        ),
        (
            crate::shader::TERRAIN_WGSL,
            (51_021, 0x5aee_0909_1af2_a2f6_1004_61bd_40ec_af61),
        ),
        (
            shadowed,
            (111_951, 0x9b08_f164_48af_3cdc_4757_d755_f47d_e47d),
        ),
        (
            shadowed_terrain,
            (69_077, 0x0182_2e98_c9cb_a155_3d10_803e_d7f9_3b46),
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
            (95_673, 0x657e_914c_ac3e_92bc_dba7_bdb7_d2d8_884e),
        ),
        (
            shadowed,
            (112_186, 0xd5f8_512a_0133_9c51_a75b_e42f_0cab_d58b),
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
        (97_026, 0x8c4a_a11b_7d34_5246_7eb2_0f26_6ae1_0327),
        (97_000, 0x9e5a_c83a_b593_81b5_b833_18a5_bbd0_f035),
        (97_261, 0xdea6_c833_5481_be8a_9409_2b48_9d79_163d),
        (97_235, 0xca28_0a93_740f_0dfe_a3cd_b6c0_e988_6c37),
    ],
    [
        (96_914, 0x2cb5_d59d_fb4d_2c8a_ed43_f81e_eeb4_826f),
        (96_888, 0x4948_d556_01fe_fe49_02fb_1c61_7358_89dd),
        (97_149, 0x49f6_13f1_a9ac_8d6a_e371_c9f6_674e_74fb),
        (97_123, 0xe82a_6c1c_cb26_13d8_67a3_edfb_1b31_61f9),
    ],
    [
        (97_052, 0x8dda_352c_1a9c_3a9b_516d_667f_807f_82d7),
        (97_026, 0x6451_83d0_c001_30be_0667_f33f_451c_ab05),
        (97_287, 0x5285_618f_a403_e148_ae1e_a0ba_fdc6_bc31),
        (97_261, 0xf955_a3d2_14fc_c1c1_937d_fe88_ef80_0eeb),
    ],
    [
        (96_994, 0xff06_306e_ba81_e4fa_a1c9_9f10_9080_7bf3),
        (96_968, 0x8521_f9ef_cb13_b813_b23f_9469_8402_2731),
        (97_229, 0xfe20_5027_ed01_5d33_c64b_5781_d95f_a3d3),
        (97_203, 0x814f_b432_b683_d3ca_c59e_6ef4_0811_e051),
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
fn directional_shadow_assembly_pins_card_geometry_first_rejection() {
    assert_eq!(
        source_fingerprint(&crate::shadows::depth_source()),
        (96_134, 0x92aa_86f4_dcf8_efc8_91f1_f51c_d274_de38),
    );
}
