@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    let depth = scene_relief_depth(input);
    let metadata = reflection_metadata(input, front, false);
    if !depth.visible { discard; }
    return SceneSurfaceOutput(metadata, depth.value);
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneSurfaceOutput {
    let depth = scene_relief_depth(input);
    let metadata = reflection_metadata(input, front, true);
    if !depth.visible { discard; }
    return SceneSurfaceOutput(metadata, depth.value);
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,false); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> { return reflection_metadata(input,front,true); }
