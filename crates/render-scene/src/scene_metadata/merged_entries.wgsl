struct SceneMetadataOutput { @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, }
struct SceneMetadataReliefOutput { @location(0) normal: vec4<f32>, @location(1) material: vec4<f32>, @builtin(frag_depth) depth: f32, }

fn scene_metadata_relief(surface: SceneMetadataOutput, input: VertexOutput) -> SceneMetadataReliefOutput {
    return SceneMetadataReliefOutput(surface.normal, surface.material, scene_relief_depth(input));
}

@fragment fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataReliefOutput {
    return scene_metadata_relief(reflection_metadata(input, front, false), input);
}
@fragment fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataReliefOutput {
    return scene_metadata_relief(reflection_metadata(input, front, true), input);
}

@fragment fn fs_opaque(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataOutput { return reflection_metadata(input,front,false); }
@fragment fn fs_masked(input: VertexOutput, @builtin(front_facing) front: bool) -> SceneMetadataOutput { return reflection_metadata(input,front,true); }
