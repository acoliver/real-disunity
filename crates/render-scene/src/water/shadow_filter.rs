//! Instantiate water's opt-in sampler from the shared cascade implementation.
pub(super) const SPECIALIZATION: u32 = 2048;

pub(crate) fn receiver_source() -> String {
    let shared = include_str!("../shader/shadows.wgsl");
    let (_, receiver) = shared
        .split_once("fn cascade_visibility")
        .expect("cascade receiver");
    let (receiver, _) = receiver
        .split_once("fn direct_sunlight_visibility")
        .expect("direct response");
    let (coverage, sampling) = receiver
        .split_once("    if shadow.bias.z < 0.0")
        .expect("receiver filter");
    let (_, selection) = sampling
        .split_once("fn single_shadow_visibility")
        .expect("single map receiver");
    // Coverage, projection, derivatives, selection and blending have one source.
    // Only the sampling expression differs; the ambient receiver stays untouched.
    let receiver = format!("fn cascade_visibility{coverage}    return rd_shadow_array_weighted_pcf5(shadow_depth, shadow_sampler, uv, layer, ndc.z - shadow.depth_bias[layer]);\n}}\n\nfn single_shadow_visibility{selection}")
        .replace("cascade_visibility", "water_filtered_cascade_visibility")
        .replace("single_shadow_visibility", "water_filtered_single_shadow_visibility")
        .replace("raw_sunlight_visibility", "water_filtered_raw_sunlight_visibility");
    format!(
        "{}\n{receiver}",
        worldrunner_render_shadow::WEIGHTED_PCF5_WGSL
    )
}

pub(super) fn source(source: &str) -> String {
    let visibility = "fn water_shadow_visibility(position: vec3<f32>, normal: vec3<f32>, shading_visibility: f32) -> f32 {";
    assert_eq!(source.matches(visibility).count(), 1);
    let source = source.replace(visibility, &format!("{visibility}\n    if (u32(water.wind_flags.w) & 4u) != 0u {{ return water_filtered_raw_sunlight_visibility(position, water_shadow_normal(normal)); }}"));
    format!("{source}\n{}", receiver_source())
}
