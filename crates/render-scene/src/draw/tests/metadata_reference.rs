//! Frozen metadata evaluation before alpha-first rejection. Synthetic GPU oracle only.
use super::*;
use std::collections::BTreeSet;

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
            crate::renderer::SceneTarget {
                format: wgpu::TextureFormat::Rgba16Float,
                samples: 1,
            },
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

/// Parses and validates `source` with every naga capability, so a rewrite that no longer
/// compiles fails here on machines without a GPU, and returns its entry points as
/// `Stage:name`. The short hash is printed for a by-eye diff of a base run against a
/// candidate run and is never asserted.
fn entry_points(label: &str, source: &str) -> BTreeSet<String> {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    println!(
        "WGSL-VARIANT label={label:?} bytes={} hash={:016x}",
        source.len(),
        crate::dynamic::capture_tests::digest(source.as_bytes())
    );
    let points: BTreeSet<String> = module
        .entry_points
        .iter()
        .map(|point| format!("{:?}:{}", point.stage, point.name))
        .collect();
    for stage in ["Vertex:", "Fragment:"] {
        assert!(
            points.iter().any(|point| point.starts_with(stage)),
            "{label} has no {stage} entry point: {points:?}"
        );
    }
    points
}

/// Runs `check` over `items` on up to four threads: validating a few hundred 100 KB sources
/// one after another takes over a minute in a debug build.
fn check_in_parallel<T: Sync>(items: &[T], check: impl Fn(&T) + Sync) {
    let chunk = items.len().div_ceil(4).max(1);
    std::thread::scope(|scope| {
        for part in items.chunks(chunk) {
            let check = &check;
            scope.spawn(move || part.iter().for_each(check));
        }
    });
}

/// Validates `source` and requires it to expose every entry point in `required`.
fn assert_exposes(label: &str, source: &str, required: &BTreeSet<String>) {
    let exposed = entry_points(label, source);
    let missing: Vec<_> = required.difference(&exposed).collect();
    assert!(missing.is_empty(), "{label} lost entry points {missing:?}");
}

#[test]
fn forward_assemblies_validate_and_expose_their_entry_points() {
    let (shadowed, shadowed_terrain) = crate::shadows::capture_sources(true);
    let object = entry_points("stock object", crate::shader::ENVIRONMENT_WGSL);
    let terrain = entry_points("stock terrain", crate::shader::TERRAIN_WGSL);
    let lighting =
        worldrunner_render_plan::SceneCelestialLighting::new(500_000, 2_000_000, 3_000_000)
            .unwrap();
    for (label, source, required) in [
        ("shadowed object", shadowed, &object),
        ("shadowed terrain", shadowed_terrain, &terrain),
    ] {
        assert_exposes(label, source, required);
    }
    for (label, source) in [
        ("lit object", crate::shader::ENVIRONMENT_WGSL),
        ("lit shadowed object", shadowed),
    ] {
        let lit = crate::sky::lighting::source(source.to_owned(), lighting);
        assert_exposes(label, &lit, &object);
    }
}

#[test]
fn celestial_forward_assemblies_validate_and_expose_their_entry_points() {
    use worldrunner_render_plan::SceneCelestialLighting;
    let required = [
        "Vertex:vs_celestial",
        "Fragment:fs_celestial_opaque",
        "Fragment:fs_celestial_masked",
    ]
    .map(str::to_owned)
    .into();
    let lightings = [
        SceneCelestialLighting::NEUTRAL,
        SceneCelestialLighting::new(500_000, 2_000_000, 3_000_000).unwrap(),
    ];
    let mut variants = Vec::new();
    for bits in 0..8_u8 {
        let composition = (bits & 4 != 0, bits & 2 != 0, bits & 1 != 0);
        for (selected, lighting) in lightings.into_iter().enumerate() {
            for anchored in [false, true] {
                variants.push((composition, selected, lighting, anchored));
            }
        }
    }
    check_in_parallel(&variants, |&(composition, selected, lighting, anchored)| {
        let source = crate::renderer::celestial_source(composition, lighting, anchored);
        let label = format!("celestial {composition:?} lighting={selected} anchored={anchored}");
        assert_exposes(&label, &source, &required);
    });
}

#[test]
fn directional_shadow_assembly_validates_and_exposes_its_entry_points() {
    let required = [
        "Vertex:vs_shadow",
        "Fragment:fs_depth_opaque",
        "Fragment:fs_depth_masked",
    ]
    .map(str::to_owned)
    .into();
    assert_exposes(
        "directional shadow caster",
        &crate::shadows::depth_source(),
        &required,
    );
}
