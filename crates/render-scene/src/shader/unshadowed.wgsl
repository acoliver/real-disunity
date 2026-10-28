fn raw_sunlight_visibility(_position: vec3<f32>, _normal: vec3<f32>) -> f32 {
    return 1.0;
}

fn direct_sunlight_visibility(visibility: f32) -> f32 {
    return visibility;
}

fn sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    return direct_sunlight_visibility(raw_sunlight_visibility(position, normal));
}
