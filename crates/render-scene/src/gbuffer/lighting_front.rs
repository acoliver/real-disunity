//! Front lighting reuses the forward screen-neighbour solve on reconstructed inputs.
//!
//! The cascade's projection arithmetic and zero-bias bit selection remain unchanged. The
//! receiver normal comes from G3. Quad displacements compatible with its quantization are
//! retained; a discontinuous displacement uses the forward terrain footprint's plane solve
//! on the current pixel's normal. This keeps another surface's depth out of that solve.

use crate::scene_metadata::receiver::function_source;

const AXES: [(&str, &str); 2] = [
    ("dpdx(position)", "vec2<f32>(1.0, 0.0)"),
    ("dpdy(position)", "vec2<f32>(0.0, 1.0)"),
];

const QUAD_PLANE: &str = "fn scene_front_on_plane(quad: vec3<f32>, normal: vec3<f32>) -> bool {
    // Oct rounding changes the raw normal by at most sqrt(6)/127. Its length is at
    // least 1/sqrt(3), giving this sine bound for the normalized 7+7-bit G3 normal.
    let normal_error = sqrt(18.0) / 127.0;
    return abs(dot(normal, quad)) <= normal_error * length(quad);
}";

/// Both raster lanes in the derivative pair must carry the same geometric normal.
const PAIR_PROVENANCE: &str =
    "fn scene_front_pair_origin(pixel: vec2<i32>, direction: vec2<i32>) -> vec2<i32> {
    return pixel - (pixel & vec2<i32>(1)) * direction;
}
fn scene_front_pair_provenance(axis: vec2<f32>) -> bool {
    let pixel = vec2<i32>(scene_deferred_fragment.xy);
    let direction = vec2<i32>(axis);
    let first = scene_front_pair_origin(pixel, direction);
    let second = first + direction;
    let size = vec2<i32>(textureDimensions(scene_deferred_g3));
    if any(first < vec2<i32>(0)) || any(second >= size) {
        return false;
    }
    let first_g3 = textureLoad(scene_deferred_g3, first, 0).r;
    let second_g3 = textureLoad(scene_deferred_g3, second, 0).r;
    return first_g3 != 0u && second_g3 != 0u
        && (first_g3 & 0xfffffffcu) == (second_g3 & 0xfffffffcu);
}";

/// Observation and pixels the producer did not cover retain the quad difference.
const DELTA: &str = "fn scene_front_receiver_delta(quad: vec3<f32>, axis: vec2<f32>) -> vec3<f32> {
    let normal = scene_deferred_geometric_normal;
    let on_plane = scene_front_on_plane(quad, normal);
    let solved = scene_front_displacement(scene_deferred_fragment, axis, normal);
    let provenance = scene_front_pair_provenance(axis);
    let plane = scene_deferred_camera.receiver_policy.x != 0u && scene_deferred_covered
        && (!provenance || !on_plane);
    return select(quad, solved, plane);
}";

/// The forward terrain footprint's plane solve, returning the world displacement of the
/// adjacent pixel of the quad pair instead of the terrain's east-north components.
fn displacement() -> String {
    let solve = function_source(crate::shader::TERRAIN_WGSL, "terrain_quad_footprint")
        .replace("terrain_quad_footprint(", "scene_front_displacement(")
        .replace(") -> vec2<f32>", ") -> vec3<f32>")
        .replace(
            "clamp(displacement.xy * direction, vec2<f32>(-1e4), vec2<f32>(1e4))",
            "clamp(displacement * direction, vec3<f32>(-1e4), vec3<f32>(1e4))",
        );
    assert!(solve.contains("-> vec3<f32>") && solve.contains("vec3<f32>(-1e4)"));
    solve
}

/// The cascade with each quad difference of the position replaced by the plane displacement.
/// The solve is evaluated before the cascade's own derivative lines so that the quad
/// differences stay in uniform control flow.
fn cascade(original: &str) -> String {
    let mut prepared = original.to_owned();
    for (axis, (hardware, direction)) in ["x", "y"].into_iter().zip(AXES) {
        let line = format!("let receiver_d{axis} = ");
        assert_eq!(prepared.matches(&line).count(), 1);
        assert_eq!(prepared.matches(hardware).count(), 2);
        prepared = prepared.replace(hardware, &format!("front_d{axis}")).replace(
            &line,
            &format!(
                "let front_d{axis} = scene_front_receiver_delta({hardware}, {direction});\n    {line}"
            ),
        );
    }
    prepared
}

pub(super) fn source(object: &str) -> String {
    const HEADER: &str =
        "fn raw_sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {";
    if !object.contains("fn raw_sunlight_visibility(") || object.contains("fn scene_receiver_load(")
    {
        return object.to_owned();
    }
    let original = function_source(object, "raw_sunlight_visibility");
    if original.contains("fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>)") {
        return object.to_owned();
    }
    assert_eq!(original.matches(HEADER).count(), 1);
    // Leave the cascade's projection arithmetic and its zero-bias bit selection intact.
    let prepared = original.replace(HEADER,
        "fn raw_sunlight_visibility(position: vec3<f32>, shading_normal: vec3<f32>) -> f32 {
    let normal = scene_deferred_receiver_normal(shading_normal, scene_deferred_geometric_normal, scene_deferred_camera.receiver_policy.x != 0u);");
    let cascaded = function_source(object, "cascaded_sunlight_visibility");
    let object = object
        .replace(original, &prepared)
        .replace(cascaded, &cascade(cascaded));
    format!(
        "{object}\n{}\n{QUAD_PLANE}\n{PAIR_PROVENANCE}\n{DELTA}\n",
        displacement()
    )
}

#[cfg(test)]
pub(super) fn cascade_with_hardware_differences(composed: &str) -> String {
    let mut original = composed.to_owned();
    for (axis, (hardware, direction)) in ["x", "y"].into_iter().zip(AXES) {
        original = original
            .replace(
                &format!(
                    "let front_d{axis} = scene_front_receiver_delta({hardware}, {direction});\n    "
                ),
                "",
            )
            .replace(&format!("front_d{axis}"), hardware);
    }
    original
}
