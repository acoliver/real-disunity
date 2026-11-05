struct SceneMetadataOutput { @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, }
struct SceneMetadataReliefOutput { @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, @builtin(frag_depth) depth: f32, }

@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataReliefOutput {
    let depth = scene_relief_depth(input);
    let surface = reflection_metadata(input, front, false);
    if !depth.visible { discard; }
    return SceneMetadataReliefOutput(surface.normal, surface.material, depth.value);
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataReliefOutput {
    let depth = scene_relief_depth(input);
    let surface = reflection_metadata(input, front, true);
    if !depth.visible { discard; }
    return SceneMetadataReliefOutput(surface.normal, surface.material, depth.value);
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataOutput { return reflection_metadata(input,front,false); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataOutput { return reflection_metadata(input,front,true); }
