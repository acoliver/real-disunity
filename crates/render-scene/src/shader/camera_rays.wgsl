fn scene_orthographic() -> bool {
    let row = transpose(frame.view_projection)[3];
    return dot(row.xyz,row.xyz)==0.0;
}
fn scene_ray_origin(position:vec3<f32>) -> vec3<f32> {
    if !scene_orthographic() { return frame.camera_position.xyz; }
    let forward=normalize(transpose(frame.view_projection)[2].xyz);
    return position-forward*dot(position-frame.camera_position.xyz,forward);
}
fn scene_background_origin(ndc:vec2<f32>) -> vec3<f32> {
    let eye=frame.camera_position.xyz;
    if !scene_orthographic() { return eye; }
    let rows=transpose(frame.view_projection);
    let clip_eye=frame.view_projection*vec4(eye,1.0);
    return eye+rows[0].xyz*((ndc.x-clip_eye.x)/dot(rows[0].xyz,rows[0].xyz))
        +rows[1].xyz*((ndc.y-clip_eye.y)/dot(rows[1].xyz,rows[1].xyz));
}
