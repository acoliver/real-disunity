struct SceneNormalPreparation {
    footprint: AtlasFootprint,
    basis: mat3x3<f32>,
}
fn scene_prepare_normal(input: VertexOutput, geometric: vec3<f32>, position: vec3<f32>) -> SceneNormalPreparation {
    var prepared: SceneNormalPreparation;
    if (material.texture_flags & FLAG_NORMAL) == 0u { return prepared; }
    prepared.footprint = scene_atlas_footprint(normal_texture, input);
    prepared.basis = cotangent_frame(geometric, position, input.texcoord);
    return prepared;
}
fn sample_normal_prepared(input: VertexOutput, geometric: vec3<f32>, tangent: vec4<f32>, prepared: SceneNormalPreparation) -> vec3<f32> {
    if (material.texture_flags & FLAG_NORMAL) == 0u { return geometric; }
    let footprint = prepared.footprint;
    var frame_basis = prepared.basis;
