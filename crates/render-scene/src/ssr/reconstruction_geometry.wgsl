fn reflection_tangent(receiver: ReflectionReceiver, pixel: vec2<i32>, axis: vec2<i32>, sample: u32) -> vec3<f32> {
    let last = vec2<i32>(camera.size.xy) - 1;
    let a_pixel = clamp(pixel - axis, vec2<i32>(0), last);
    let b_pixel = clamp(pixel + axis, vec2<i32>(0), last);
    let a = reflection_surface(a_pixel, sample);
    let b = reflection_surface(b_pixel, sample);
    let a_valid = a.owner == receiver.owner && any(a_pixel != pixel);
    let b_valid = b.owner == receiver.owner && any(b_pixel != pixel);
    let da = receiver.position - a.position;
    let db = b.position - receiver.position;
    if !a_valid && !b_valid { return vec3<f32>(0.0); }
    // Prefer the closer same-owner neighbour so a silhouette or depth step
    // does not tilt the local plane toward an unrelated surface.
    return select(da, db, b_valid && (!a_valid || dot(db, db) < dot(da, da)));
}
fn reflection_receiver(p: vec2<i32>, sample: u32) -> ReflectionReceiver {
    var receiver = reflection_surface(p, sample);
    if receiver.owner == 0u { return receiver; }
    let x = reflection_tangent(receiver, p, vec2<i32>(1, 0), sample);
    let y = reflection_tangent(receiver, p, vec2<i32>(0, 1), sample);
    let plane = cross(x, y);
    // A one-pixel-wide surface has no depth stencil defining a plane.
    // Its published normal is the remaining local surface estimate.
    receiver.plane = receiver.normal;
    if dot(plane, plane) > 0.0 { receiver.plane = normalize(plane); }
    return receiver;
}
