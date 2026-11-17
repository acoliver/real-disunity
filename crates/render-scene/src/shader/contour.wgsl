struct SceneContourId { value: vec4<u32>, }
@group(2) @binding(30) var<uniform> scene_contour_id: SceneContourId;

// Coverage is recomputed, but the completed depth owns the MSAA sample mask.
fn scene_contour_visible(original: VertexOutput, position: vec3<f32>, masked: bool, visible: bool) {
    var input = original;
    input.world_position = position;
    let alpha = scene_sampled_alpha(input);
    let coverage = scene_mask_alpha(alpha, input);
    if !visible || !scene_geometry_covered(input) { discard; }
    if masked {
        if SCENE_ALPHA_TO_COVERAGE {
            if coverage == 0.0 { discard; }
        } else {
            if !scene_alpha_covered(alpha, input) { discard; }
        }
    }
}

@fragment fn fs_contour_opaque(input: VertexOutput) -> @location(0) u32 {
    scene_contour_visible(input, scene_relief_position(input), false, true);
    return scene_contour_id.value.x;
}
@fragment fn fs_contour_masked(input: VertexOutput) -> @location(0) u32 {
    scene_contour_visible(input, scene_relief_position(input), true, true);
    return scene_contour_id.value.x;
}
struct SceneContourDepth { @location(0) id: u32, @builtin(frag_depth) depth: f32, }
@fragment fn fs_contour_relief_opaque(input: VertexOutput) -> SceneContourDepth {
    let depth = scene_relief_depth(input);
    scene_contour_visible(input, depth.position, false, depth.visible);
    return SceneContourDepth(scene_contour_id.value.x, depth.value);
}
@fragment fn fs_contour_relief_masked(input: VertexOutput) -> SceneContourDepth {
    let depth = scene_relief_depth(input);
    scene_contour_visible(input, depth.position, true, depth.visible);
    return SceneContourDepth(scene_contour_id.value.x, depth.value);
}
