@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    return SceneSurfaceOutput(reflection_metadata(input, front, false),scene_relief_depth(input));
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    return SceneSurfaceOutput(reflection_metadata(input, front, true),scene_relief_depth(input));
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,false); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,true); }
