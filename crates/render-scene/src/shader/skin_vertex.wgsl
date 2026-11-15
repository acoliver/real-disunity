// The skinned form of the object vertex stage, composed after geometry.wgsl and skinning.wgsl.
//
// The skin word and the instance descriptor are vertex inputs of the entry point alone, so the
// other entry points of the module keep their inputs (`vs_unlit` already uses location 9). The
// entry point stores them in a private variable and `transform_vertex` deforms its vertex
// through `scene_skin_local` before the model transform, which covers every pass that places
// geometry through it.

@group(1) @binding(11) var<storage, read> scene_skin_palette: array<SceneSkinRow>;

struct SceneSkinInput {
    @location(9) word: vec2<u32>,
    // (current entry offset, previous entry offset, joint count, flags)
    @location(15) descriptor: vec4<u32>,
}

var<private> scene_skin_input: SceneSkinInput;

// The vertex deformed by the current palette. Normals and tangents take the linear part of the
// blend; `scene_normal` renormalizes the normal and the tangent keeps its stored handedness.
fn scene_skin_local(source: VertexInput) -> VertexInput {
    var vertex = source;
    let offset = scene_skin_input.descriptor.x;
    let word = scene_skin_input.word;
    vertex.position = scene_skin_point(offset, word, source.position);
    vertex.normal = scene_skin_direction(offset, word, source.normal);
    vertex.tangent = vec4<f32>(scene_skin_direction(offset, word, source.tangent.xyz), source.tangent.w);
    return vertex;
}
