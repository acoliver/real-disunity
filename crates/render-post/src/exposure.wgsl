@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> bins: array<atomic<u32>, 256>;
struct Exposure { value: f32, initialized: u32, pad: vec2<u32> }
@group(0) @binding(2) var<storage, read_write> exposure: Exposure;
struct Frame { delta: f32, reset: u32, initial: f32, pad: f32 }
@group(0) @binding(3) var<uniform> frame: Frame;
override LOG_MIN: f32 = -12.0;
override LOG_MAX: f32 = 12.0;
override LOW: f32 = 0.01;
override HIGH: f32 = 0.99;
override KEY: f32 = 0.18;
override MIN_EXPOSURE: f32 = 0.01;
override MAX_EXPOSURE: f32 = 64.0;
override SPEED_UP: f32 = 3.0;
override SPEED_DOWN: f32 = 1.0;
override FIXED_EXPOSURE: f32 = -1.0;
fn finite_positive(v: f32) -> f32 {
    let b = bitcast<u32>(v);
    if (b & 0x80000000u) != 0u || (b & 0x7f800000u) == 0x7f800000u { return 0.0; }
    return v;
}
@compute @workgroup_size(8, 8)
fn histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(source)) { return; }
    let c = textureLoad(source, vec2<i32>(id.xy), 0).rgb;
    let l = dot(vec3<f32>(finite_positive(c.r), finite_positive(c.g), finite_positive(c.b)), vec3<f32>(0.2126, 0.7152, 0.0722));
    let t = clamp((log2(max(l, exp2(LOG_MIN))) - LOG_MIN) / (LOG_MAX - LOG_MIN), 0.0, 1.0);
    atomicAdd(&bins[min(u32(t * 256.0), 255u)], 1u);
}
@compute @workgroup_size(1)
fn reduce() {
    var total = 0u;
    for (var i = 0u; i < 256u; i++) { total += atomicLoad(&bins[i]); }
    let lo = f32(total) * LOW;
    let hi = f32(total) * HIGH;
    var prefix = 0.0;
    var weighted = 0.0;
    var retained = 0.0;
    for (var i = 0u; i < 256u; i++) {
        let count = f32(atomicLoad(&bins[i]));
        let weight = max(0.0, min(prefix + count, hi) - max(prefix, lo));
        weighted += weight * (LOG_MIN + (f32(i) + 0.5) * (LOG_MAX - LOG_MIN) / 256.0);
        retained += weight;
        prefix += count;
    }
    let desired = clamp(KEY / exp2(weighted / max(retained, 0.000001)), MIN_EXPOSURE, MAX_EXPOSURE);
    var previous = exposure.value;
    if exposure.initialized == 0u || frame.reset != 0u { previous = clamp(frame.initial, MIN_EXPOSURE, MAX_EXPOSURE); }
    let speed = select(SPEED_DOWN, SPEED_UP, desired > previous);
    exposure.value = mix(previous, desired, 1.0 - exp(-speed * frame.delta));
    if FIXED_EXPOSURE >= 0.0 { exposure.value = FIXED_EXPOSURE; }
    exposure.initialized = 1u;
}
