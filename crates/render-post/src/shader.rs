/// Composable generic post kernels. Coefficients and transforms are supplied by the environment.
#[macro_export]
macro_rules! post_wgsl {
    () => { r"
fn rd_post_exposure(color: vec3<f32>, multiplier: f32) -> vec3<f32> {
    return color * multiplier;
}
fn rd_rational_curve(value: vec3<f32>, numerator: vec3<f32>, denominator: vec3<f32>) -> vec3<f32> {
    return (value * (numerator.x * value + numerator.y) + numerator.z)
        / (value * (denominator.x * value + denominator.y) + denominator.z);
}
// Matrix vectors are rows, rather than WGSL's native column-vector multiplication.
fn rd_transform_rows(rows: mat3x3<f32>, color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(rows[0], color), dot(rows[1], color), dot(rows[2], color));
}
fn rd_display_power(color: vec3<f32>, exponent: vec3<f32>) -> vec3<f32> {
    return pow(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), exponent);
}
" };
}
