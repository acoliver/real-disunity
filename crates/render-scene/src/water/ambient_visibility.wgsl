// Keep water AO sampling at the surface-lighting evaluation site.
fn scene_surface_ambient_visibility(world_position: vec3<f32>, normal: vec3<f32>, ambient: f32) -> f32 {
    return raw_sunlight_visibility(world_position, normal);
}
