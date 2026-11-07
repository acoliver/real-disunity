// Decode source channels before scaling tangent-space XY.
fn sample_normal(input: VertexOutput, geometric: vec3<f32>, position: vec3<f32>, tangent: vec4<f32>) -> vec3<f32> {
    if (material.texture_flags & FLAG_NORMAL) == 0u {
        return geometric;
    }
    let footprint = scene_atlas_footprint(normal_texture, input);
    var frame_basis = cotangent_frame(geometric, position, input.texcoord);
