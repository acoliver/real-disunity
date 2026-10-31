// Shared by taa.wgsl and taa_upscale.wgsl, which are built as this file followed by the stage.
const MIN_BOX_EXTENT: f32 = 1e-4;
const MAX_COMPRESSED_LUMA: f32 = 0.99999;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
// WGSL does not promise NaN or infinity comparisons, so test the exponent bits.
fn finite(v: f32) -> bool {
    return (bitcast<u32>(v) & 0x7f800000u) != 0x7f800000u;
}
fn clean(v: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        select(0.0, max(v.x, 0.0), finite(v.x)),
        select(0.0, max(v.y, 0.0), finite(v.y)),
        select(0.0, max(v.z, 0.0), finite(v.z)),
        select(0.0, max(v.w, 0.0), finite(v.w)),
    );
}
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, LUMA);
}
fn compress(c: vec3<f32>) -> vec3<f32> {
    return c / (1.0 + luma(c));
}
fn decompress(c: vec3<f32>) -> vec3<f32> {
    return c / (1.0 - clamp(luma(c), 0.0, MAX_COMPRESSED_LUMA));
}
fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
        0.5 * c.r - 0.5 * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b,
    );
}
fn from_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}
// Moves `h` toward the box centre `m` along the line between them until it is inside the box
// of half-extent `e`, instead of clamping each channel.
fn clip_toward_centre(h: vec3<f32>, m: vec3<f32>, e: vec3<f32>) -> vec3<f32> {
    let offset = h - m;
    let ratio = abs(offset) / max(e, vec3<f32>(MIN_BOX_EXTENT));
    let t = max(max(ratio.x, ratio.y), max(ratio.z, 1.0));
    return m + offset / t;
}
