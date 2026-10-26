/// Literal expansion permits static shader composition without a filesystem or runtime loader.
#[macro_export]
macro_rules! lighting_wgsl {
    () => { r"
// Universal kernels: caller supplies data, basis, algorithm and numerical guards.
fn rd_distribution_ggx(no_h: f32, roughness: f32, epsilon: f32, guard_normalized_denominator: bool) -> f32 {
    let alpha = roughness * roughness;
    let raw_denominator = no_h * no_h * (alpha * alpha - 1.0) + 1.0;
    if guard_normalized_denominator {
        return alpha * alpha / max(3.141592653589793 * raw_denominator * raw_denominator, epsilon);
    }
    let denominator = max(raw_denominator, epsilon);
    let ratio = alpha / denominator;
    return ratio * ratio / 3.141592653589793;
}
// algorithm 0: height-correlated Smith; 1: Schlick-Smith with hotness reduction.
fn rd_visibility(no_v: f32, no_l: f32, roughness: f32, algorithm: u32) -> f32 {
    if algorithm != 0u {
        let k = max(select((roughness + 1.0) * (roughness + 1.0) * 0.125, roughness * roughness * 0.5, algorithm == 2u), 1e-5);
        return 0.25 / ((no_l * (1.0-k) + k) * (no_v * (1.0-k) + k));
    }
    let a2 = roughness * roughness * roughness * roughness;
    let v = no_l * sqrt(no_v * no_v * (1.0-a2) + a2);
    let l = no_v * sqrt(no_l * no_l * (1.0-a2) + a2);
    return 0.5 / max(v+l, 1e-7);
}
fn rd_remap_roughness(value: f32, remap: u32) -> f32 {
    if remap == 2u { return sqrt(clamp(value, 0.0, 1.0)); }
    return value;
}
fn rd_specular_occlusion(no_v: f32, ao: f32, roughness: f32) -> f32 {
    return clamp(pow(abs(no_v + ao), exp2(-16.0 * roughness - 1.0)) - 1.0 + ao, 0.0, 1.0);
}
fn rd_split_sum(f0: vec3<f32>, coefficients: vec2<f32>) -> vec3<f32> {
    return f0 * coefficients.x + vec3<f32>(coefficients.y);
}
fn rd_fresnel_schlick(cosine: f32, f0: vec3<f32>) -> vec3<f32> {
    let t = clamp(1.0-cosine, 0.0, 1.0);
    return f0 + (vec3<f32>(1.0)-f0) * t*t*t*t*t;
}
fn rd_hemisphere(normal: vec3<f32>, up: vec3<f32>, ground: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    return mix(ground, sky, dot(normal, up)*0.5+0.5);
}
fn rd_toward_light(direction: vec3<f32>, is_light_travel: bool) -> vec3<f32> {
    return normalize(select(direction, -direction, is_light_travel));
}
fn rd_apply_exposure(radiance: vec3<f32>, multiplier: f32) -> vec3<f32> {
    return radiance * multiplier;
}
fn rd_exposure_from_stops(stops: f32) -> f32 { return exp2(stops); }
// Coefficients multiply [1,x,y,z,xy,xz,yz,x²-y²,z²] in an explicitly supplied frame.
fn rd_irradiance_polynomial(direction: vec3<f32>, c: array<vec3<f32>, 9>) -> vec3<f32> {
    let x = direction.x;
    let y = direction.y;
    let z = direction.z;
    return c[0] + c[1]*x + c[2]*y + c[3]*z + c[4]*(x*y)
        + c[5]*(x*z) + c[6]*(y*z) + c[7]*(x*x-y*y) + c[8]*(z*z);
}
// The opaque input already contains camera fog; never invert a dense segment.
fn rd_compose_water_fog(snapshot: vec3<f32>, transmission: vec3<f32>, scattering: vec3<f32>, specular: vec3<f32>, air: f32, fog: vec3<f32>) -> vec3<f32> {
    return snapshot * transmission + (scattering * air + fog * (1.0 - air)) * (vec3<f32>(1.0) - transmission) + specular * air;
}
// Exponential extinction along a segment, with the horizontal-ray analytic limit.
// log_density_at_eye is logarithmic extinction per metre; all distances are metres.
fn rd_height_fog_transmittance(log_density_at_eye:f32,falloff:f32,start_distance:f32,offset:vec3<f32>,up:vec3<f32>)->f32 {
    let length=length(offset);
    if length<=start_distance { return 1.0; }
    let ray=offset/length;
    let distance=length-start_distance;
    let x=falloff*dot(ray,up)*distance;
    var log_integral=log(1.0-x*0.5+x*x/6.0-x*x*x/24.0);
    if x < -50.0 { log_integral=-x+log(1.0-exp(x))-log(-x); }
    else if abs(x)>=0.001 { log_integral=log((1.0-exp(-x))/x); }
    let log_depth=log_density_at_eye-falloff*dot(ray,up)*start_distance+log(distance)+log_integral;
    if log_depth>=5.0 { return 0.0; }
    return exp(-exp(log_depth));
}

" };
}
