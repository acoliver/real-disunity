struct SceneDecalGradients { world_dx: vec3<f32>, world_dy: vec3<f32>, }
fn scene_prepare_decals(position: vec3<f32>) -> SceneDecalGradients {
    return SceneDecalGradients(dpdx(position), dpdy(position));
}
fn scene_project_decals_prepared(position: vec3<f32>, geometric: vec3<f32>, receiver: u32,
    albedo: vec3<f32>, properties: vec2<f32>, normal: vec3<f32>, gradients: SceneDecalGradients) -> SceneDecalSurface {
    var result = SceneDecalSurface(albedo, properties, normal);
    let world_dx = gradients.world_dx;
    let world_dy = gradients.world_dy;
