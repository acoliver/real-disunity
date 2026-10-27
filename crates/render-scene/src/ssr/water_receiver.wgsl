// Negative ownership encodes a coloured F0 without another receiver attachment.
fn water_receiver_normal(packed: vec4<f32>) -> vec4<f32> {
    if packed.a >= 0.0 { return packed; }
    var normal = vec3<f32>(packed.xy, 1.0 - abs(packed.x) - abs(packed.y));
    let fold = clamp(-normal.z, 0.0, 1.0);
    normal.x += select(fold, -fold, normal.x >= 0.0);
    normal.y += select(fold, -fold, normal.y >= 0.0);
    return vec4<f32>(normalize(normal), 1.0);
}
fn water_receiver_f0(normal: vec4<f32>, material: vec4<f32>) -> vec3<f32> {
    if normal.a >= 0.0 { return vec3<f32>(material.b); }
    return vec3<f32>(normal.b, material.b, -normal.a - 1.0);
}
