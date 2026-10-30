// floor((pixel + 0.5) * extent / size - 0.5) in exact integer arithmetic: the
// lower-left texel of the 2x2 AO taps around a pixel centre. Vulkan allows 2.5
// ULP of division error, so the float form can land just below an integer when
// the grid is aligned with the frame (extent == size) and floor would then
// select the neighbouring taps. Integer division is exact on every device.
fn gather_origin(pixel: vec2<i32>, size: vec2<i32>, extent: vec2<i32>) -> vec2<i32> {
    let numerator = (2 * pixel + 1) * extent - size;
    let denominator = 2 * size;
    let truncated = numerator / denominator;
    return truncated - select(vec2<i32>(0), vec2<i32>(1), numerator % denominator < vec2<i32>(0));
}
