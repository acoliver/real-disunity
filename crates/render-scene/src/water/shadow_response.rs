use std::borrow::Cow;
use worldrunner_render_plan::{SceneWaterBody, SceneWaterShadowOffset, SceneWaterShadowResponse};

pub(super) const SPECIALIZATION: u32 = 1024;

pub(super) fn key(body: &SceneWaterBody) -> u32 {
    u32::from(
        body.material
            .shadow_response
            .is_some_and(|v| v != SceneWaterShadowResponse::default()),
    ) * SPECIALIZATION
}

pub(super) fn uniforms(body: &SceneWaterBody, data: &mut [f32; 32]) {
    if key(body) == 0 {
        return;
    }
    let response = body.material.shadow_response.expect("enabled response");
    data[15] = match response.offset {
        SceneWaterShadowOffset::ShadingNormal => 1.0,
        SceneWaterShadowOffset::GeometricNormal => 2.0,
        SceneWaterShadowOffset::None => 3.0,
    };
    data[30] = super::number(i64::from(response.diffuse_floor_micro)) / 1000.0;
    data[31] =
        f32::from(u8::from(response.depth_relief) + 2 * u8::from(response.disk_raw_visibility));
}

pub(super) fn specialize(source: &str, terms: u32) -> Cow<'_, str> {
    if terms & SPECIALIZATION == 0 {
        return Cow::Borrowed(source);
    }
    let source = source.replace(
        "let visibility = sunlight_visibility(input.world, normal);",
        "let raw_visibility = sunlight_visibility(input.world, water_shadow_normal(normal));\n    let shadow_depth = water_shadow_depth(original_distance);\n    let visibility = water_shadow_response(raw_visibility, shadow_depth);",
    ).replace(
        "specular += water_disk_highlight(normal, view, light, material_f0, roughness, visibility);",
        "specular += water_shadow_disk(water_disk_highlight(normal, view, light, material_f0, roughness, visibility), raw_visibility);",
    ).replace(
        "vec3<f32>(roughness, water.terms.z, 1.0), lighting_f0);",
        "vec3<f32>(roughness, water.terms.z, 1.0), lighting_f0, shadow_depth);",
    );
    Cow::Owned(format!(
        "{}\n{}\n{}",
        specialize_surface(&source),
        include_str!("shadow_response.wgsl"),
        LOOKUP
    ))
}

fn specialize_surface(source: &str) -> String {
    source.replace(
// Ambient occlusion retains its shading-normal visibility when the direct lookup changes.
        "dielectric_f0: vec3<f32>) -> SceneSurfaceLight",
        "dielectric_f0: vec3<f32>, shadow_depth: f32) -> SceneSurfaceLight",
    ).replace(
        "vec3<f32>(roughness_in, metalness, ambient_occlusion), DIELECTRIC_F0).color;",
        "vec3<f32>(roughness_in, metalness, ambient_occlusion), DIELECTRIC_F0, 0.0).color;",
    ).replace(
        "raw_sunlight_visibility(world_position, normal);",
        "raw_sunlight_visibility(world_position, water_shadow_normal(normal));",
    ).replace(
        "let ao = scene_ao(ambient_occlusion, world_position, normal_dot_light, visibility);",
        "var ambient_visibility = visibility;
    if SCENE_SPECULAR_OCCLUSION != 0u && water.disk.w >= 2.0 { ambient_visibility = raw_sunlight_visibility(world_position, normal); }
    let ao = scene_ao(ambient_occlusion, world_position, normal_dot_light, ambient_visibility);",
    ).replace(
        "* normal_dot_light * shadow + ambient;",
        "* normal_dot_light * water_shadow_response(shadow, shadow_depth) + ambient;",
    ).replace(
        "scene_disk_highlight(normal, view, light, f0, roughness_in, shadow) * specular_ao;",
        "water_shadow_disk(scene_disk_highlight(normal, view, light, f0, roughness_in, water_shadow_response(shadow, shadow_depth)), shadow) * specular_ao;",
    )
}

const LOOKUP: &str = include_str!("shadow_lookup.wgsl");
