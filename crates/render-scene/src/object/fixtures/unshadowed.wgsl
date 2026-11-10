fn sunlight_normal_bias() -> f32 { return 0.0; }
fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {
    return 1.0;
}

fn direct_sunlight_visibility(visibility: f32) -> f32 {
    return visibility;
}

fn sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    return direct_sunlight_visibility(raw_sunlight_visibility(position, normal));
}
struct SceneCardShadow { unused: f32, }
fn scene_prepare_card_shadow(_position: vec3<f32>, _normal: vec3<f32>) -> SceneCardShadow {
    return SceneCardShadow(0.0);
}
fn scene_card_shadow_visibility(_position: vec3<f32>, _prepared: SceneCardShadow) -> f32 {
    return 1.0;
}
