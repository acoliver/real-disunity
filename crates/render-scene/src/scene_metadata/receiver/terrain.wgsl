var<private> scene_receiver_dx: vec3<f32>;
var<private> scene_receiver_dy: vec3<f32>;

fn scene_prepare_terrain_receiver(input: TerrainVertexOutput) {
    let plane = normalize(terrain_face_normal(input.world_position.xy, input.plane_index, input.skirt_axis));
    scene_receiver_dx = scene_receiver_displacement(input.clip_position, vec2<f32>(1.0, 0.0), plane);
    scene_receiver_dy = scene_receiver_displacement(input.clip_position, vec2<f32>(0.0, 1.0), plane);
}
