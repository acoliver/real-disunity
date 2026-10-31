// Replays cd26906e's absent-response water equations without Git at test runtime.
// Apple M4 Max/Metal, RGBA16Float, 256x256, 4x MSAA yielded FNV-1a64
// 9614914860580183008 (shared) and 16396179683596547389 (local).
// Compute the reference on the active adapter rather than assuming those bytes
// are portable across shader compilers and GPU architectures.
pub(super) fn source() -> String {
    let mut source = crate::water::water_source(true);
    let start = source.find("struct SceneSurfaceLight {").unwrap();
    let end = start + source[start..].find('}').unwrap() + 1;
    source.replace_range(
        start..end,
        "struct SceneSurfaceLight { color: vec3<f32>, diffuse: vec3<f32>, specular: vec3<f32>, }",
    );
    let start = source.find("fn scene_surface_lighting(").unwrap();
    let end = source.find("\nfn apply_exposure").unwrap();
    source.replace_range(start..end, include_str!("base_surface.wgsl"));
    source = replace_once(
        &source,
        "    let ambient_visibility = raw_sunlight_visibility(input.world, normal);\n    let raw_visibility = water_shadow_visibility(input.world, normal, ambient_visibility);\n    let adjusted_visibility = direct_sunlight_visibility(raw_visibility);\n    let visibility = water_shadow_response(adjusted_visibility, water_shadow_depth(original_distance));",
        "    let visibility = sunlight_visibility(input.world, normal);",
    );
    source = replace_once(
        &source,
        "scene_surface_lighting_with_visibility(input.world, normal, base_color,\n            vec3<f32>(roughness, water.terms.z, 1.0), lighting_f0,\n            SceneSurfaceVisibility(raw_visibility, ambient_visibility, visibility, water_shadow_disk_factor(adjusted_visibility)))",
        "scene_surface_lighting(input.world, normal, base_color,\n            vec3<f32>(roughness, water.terms.z, 1.0), lighting_f0)",
    );
    replace_once(
        &source,
        "water_shadow_disk(water_disk_highlight(normal, view, light, material_f0, roughness, visibility), adjusted_visibility)",
        "water_disk_highlight(normal, view, light, material_f0, roughness, visibility)",
    )
}

fn replace_once(source: &str, needle: &str, replacement: &str) -> String {
    assert_eq!(source.matches(needle).count(), 1, "{needle}");
    source.replace(needle, replacement)
}

pub(super) fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}
