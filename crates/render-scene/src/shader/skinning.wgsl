// Skinning for the skinned geometry layout.
//
// A palette entry is the whole skinning transform of one joint as a row-major 3x4 matrix in
// metres (render-scene skinning.rs). The skin word holds four joint bytes in `x` and four
// normalized weight bytes in `y`; the weights of a weighted vertex sum to exactly one, so the
// blend is affine. A `y` of zero marks an explicitly unweighted vertex, which keeps its bind
// position. The palette array itself is declared by the composing program.

struct SceneSkinRow {
    x: vec4<f32>,
    y: vec4<f32>,
    z: vec4<f32>,
}

fn scene_skin_joint(word: vec2<u32>, lane: u32) -> u32 {
    return (word.x >> (8u * lane)) & 0xffu;
}

fn scene_skin_apply(row: SceneSkinRow, value: vec4<f32>) -> vec3<f32> {
    return vec3<f32>(dot(row.x, value), dot(row.y, value), dot(row.z, value));
}

// Blends `value` (w = 1 for a point, 0 for a direction) through the palette at `offset`.
fn scene_skin_blend(offset: u32, word: vec2<u32>, value: vec4<f32>) -> vec3<f32> {
    if word.y == 0u {
        return value.xyz;
    }
    let weights = unpack4x8unorm(word.y);
    var out = vec3<f32>(0.0);
    for (var lane = 0u; lane < 4u; lane += 1u) {
        let row = scene_skin_palette[offset + scene_skin_joint(word, lane)];
        out += weights[lane] * scene_skin_apply(row, value);
    }
    return out;
}

fn scene_skin_point(offset: u32, word: vec2<u32>, position: vec3<f32>) -> vec3<f32> {
    return scene_skin_blend(offset, word, vec4<f32>(position, 1.0));
}

// Directions keep their length class: normals and tangents are renormalized by the caller's
// existing world-space normalization, so only the linear part is blended here.
fn scene_skin_direction(offset: u32, word: vec2<u32>, direction: vec3<f32>) -> vec3<f32> {
    return scene_skin_blend(offset, word, vec4<f32>(direction, 0.0));
}
