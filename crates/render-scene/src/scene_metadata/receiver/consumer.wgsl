@group(2) @binding(3) var receiver_a: texture_2d_array<f32>;
@group(2) @binding(4) var receiver_b: texture_2d_array<f32>;
var<private> scene_receiver_pixel: vec2<i32>;
struct SceneReceiverDisplacements { dx: vec3<f32>, dy: vec3<f32>, }
fn scene_receiver_load(side: i32) -> SceneReceiverDisplacements {
    let a = textureLoad(receiver_a, scene_receiver_pixel, side, 0);
    let b = textureLoad(receiver_b, scene_receiver_pixel, side, 0);
    return SceneReceiverDisplacements(a.xyz, vec3<f32>(a.w, b.xy));
}
fn scene_receiver_back_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    return direct_sunlight_visibility(scene_receiver_back_raw(position, normal));
}
