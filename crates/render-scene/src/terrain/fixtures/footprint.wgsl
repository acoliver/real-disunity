fn layered_principal_axes(dx: vec2<f32>, dy: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    let scale = max(max(abs(dx.x), abs(dx.y)), max(abs(dy.x), abs(dy.y)));
    if scale <= 1e-20 / max(size.x, size.y) { return vec4<f32>(1.0, 0.0, 0.0, 0.0); }
    // Scale before the texel conversion too, so finite UV gradients cannot overflow it.
    // A power-of-two scale matches CPU division without a subnormal GPU reciprocal.
    let exponent = frexp(scale).exp - 1;
    let power_scale = ldexp(1.0, exponent);
    let east = ldexp(dx, vec2<i32>(-exponent)) * size;
    let north = ldexp(dy, vec2<i32>(-exponent)) * size;
    let texel_max = max(max(abs(east.x), abs(east.y)), max(abs(north.x), abs(north.y)));
    let texel_exponent = frexp(texel_max).exp - 1;
    let texel_scale = ldexp(1.0, texel_exponent);
    let x = ldexp(east, vec2<i32>(-texel_exponent));
    let y = ldexp(north, vec2<i32>(-texel_exponent));
    let a = x.x * x.x + y.x * y.x;
    let b = x.x * x.y + y.x * y.y;
    let c = x.y * x.y + y.y * y.y;
    let eigenvalue = 0.5 * (a + c + length(vec2<f32>(a - c, 2.0 * b)));
    let major = sqrt(eigenvalue);
    // det(J) / rho_max avoids cancellation in the smaller eigenvalue.
    let minor = abs(x.x * y.y - x.y * y.x) / major;
    var axis = vec2<f32>(1.0, 0.0);
    if a < c { axis = vec2<f32>(0.0, 1.0); }
    if b != 0.0 {
        let angle = 0.5 * atan2(2.0 * b, a - c);
        axis = vec2<f32>(cos(angle), sin(angle));
    }
    // Singular lengths beyond f32's range saturate without corrupting the unit axis.
    let lengths = vec2<f32>(major, minor) * texel_scale;
    let restored = lengths * min(vec2<f32>(power_scale), vec2<f32>(3.402823466e38) / max(lengths, vec2<f32>(1.0)));
    return vec4<f32>(axis, min(restored, vec2<f32>(3.402823466e38)));
}
fn layered_footprint(dx: vec2<f32>, dy: vec2<f32>, size: vec2<f32>, limit: f32) -> vec4<f32> {
    return layered_axis_footprint(layered_principal_axes(dx, dy, size), size, limit);
}
fn layered_axis_footprint(axes: vec4<f32>, size: vec2<f32>, limit: f32) -> vec4<f32> {
    let major = axes.z;
    let minor = axes.w;
    let direction = axes.xy * (major / size);
    if major <= 1.0 { return vec4<f32>(direction, 1.0, 0.0); }
    let count = u32(clamp(ceil(major / max(minor, 1e-20)), 1.0, limit));
    let lod = log2(max(max(major / f32(count), minor), 1e-20));
    return vec4<f32>(direction, f32(count), max(lod, 0.0));
}
