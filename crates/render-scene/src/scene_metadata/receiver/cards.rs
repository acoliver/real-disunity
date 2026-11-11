//! Card rejection is only reordered in the paired receiver consumer.
use super::shader::function;

pub(super) fn source(source: &str) -> String {
    let body = function(source, "scene_card_shade");
    let rejection = &body[body.find("    if !visible ||").unwrap()
        ..body.find("    let sampled_occlusion =").unwrap()];
    let geometric =
        "let geometric_normal = normalize(input.world_normal) * select(-1.0, 1.0, front_facing);";
    let preparation = format!(
        "{geometric}\n    let surface_footprint = scene_prepare_surface_channels(input);\n    let normal_preparation = scene_prepare_normal(input, geometric_normal, input.world_position);\n    let decal_gradients = scene_prepare_decals(input.world_position);\n    let occlusion_footprint = scene_prepare_card_occlusion(input);\n    let emission_footprint = scene_prepare_card_emission(input);\n    "
    );
    let albedo = "var albedo_sample = scene_albedo(scene_atlas_sample(albedo_texture, input, albedo_footprint), input.tint);";
    let changed = body
        .replace(rejection, "")
        .replace("    let coverage_alpha = scene_mask_alpha(albedo_sample.a, input);", "")
        .replace(geometric, "")
        .replace("    let occlusion_footprint = scene_prepare_card_occlusion(input);", "")
        .replace("    let emission_footprint = scene_prepare_card_emission(input);", "")
        .replace(albedo, &format!("{preparation}{albedo}\n    let coverage_alpha = scene_mask_alpha(albedo_sample.a, input);\n{rejection}"))
        .replace("let surface = scene_surface_channels(input);", "let surface = scene_surface_channels_prepared(input, surface_footprint);")
        .replace("sample_normal(input, geometric_normal, input.world_position, input.world_tangent)", "sample_normal_prepared(input, geometric_normal, input.world_tangent, normal_preparation)")
        .replace("scene_project_decals(input.world_position, geometric_normal, 1u,", "scene_project_decals_prepared(input.world_position, geometric_normal, 1u,")
        .replace("vec2<f32>(roughness, decal_metalness), normal);", "vec2<f32>(roughness, decal_metalness), normal, decal_gradients);");
    format!(
        "{}\n{}",
        source.replace(body, &changed),
        normal_inputs(source)
    )
}

fn normal_inputs(source: &str) -> String {
    // Moving the evaluated frame across albedo changes Metal rounding; move only its operands.
    let mut preparation = super::super::output::PREPARATION
        .replace("basis: mat3x3<f32>,", "position_dx: vec3<f32>, position_dy: vec3<f32>, uv_dx: vec2<f32>, uv_dy: vec2<f32>,")
        .replace("prepared.basis = cotangent_frame(geometric, position, input.texcoord);", "prepared.position_dx = dpdx(position); prepared.position_dy = dpdy(position); prepared.uv_dx = dpdx(input.texcoord); prepared.uv_dy = dpdy(input.texcoord);")
        .replace("var frame_basis = prepared.basis;", "var frame_basis = scene_card_cotangent_frame(geometric, prepared);");
    let mut cotangent = function(source, "cotangent_frame").replace(
        "fn cotangent_frame(normal: vec3<f32>, position: vec3<f32>, uv: vec2<f32>)",
        "fn scene_card_cotangent_frame(normal: vec3<f32>, prepared: SceneNormalPreparation)",
    );
    for (value, operand) in [
        ("position_dx", "dpdx(position)"),
        ("position_dy", "dpdy(position)"),
        ("uv_dx", "dpdx(uv)"),
        ("uv_dy", "dpdy(uv)"),
    ] {
        cotangent = cotangent.replace(operand, &format!("prepared.{value}"));
    }
    preparation.push_str(&cotangent);
    preparation
}

#[cfg(test)]
pub(crate) fn reference(source: &str) -> String {
    let body = function(source, "scene_card_shade");
    let baseline = include_str!("../../shader/card_rejection_reference.wgsl")
        .trim_end()
        .replace("var input = original;", "scene_receiver_pixel = vec2<i32>(original.clip_position.xy);\n    var input = original;")
        .replace("scene_prepare_card_shadow(input.world_position, -projected.normal)", "scene_receiver_back_card(input.world_position, -projected.normal)");
    source.replace(body, &baseline)
}
