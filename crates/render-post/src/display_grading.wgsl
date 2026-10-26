override LINEAR_SCALE: f32 = 1.0;
override POWER_R: f32 = 1.0;
override POWER_G: f32 = 1.0;
override POWER_B: f32 = 1.0;
override VIGNETTE: bool = false;
override AXIS_X: f32 = 1.0;
override AXIS_Y: f32 = 1.0;
override VG_VG_TINT_BEGIN: f32 = 0.0;
override VG_TINT_END: f32 = 1.0;
override VG_TINT_R: f32 = 0.0;
override VG_TINT_G: f32 = 0.0;
override VG_TINT_B: f32 = 0.0;
override VG_TINT_A: f32 = 0.0;
override DARK_BEGIN: f32 = 1.0;
override DARK_END: f32 = 0.0;
override DARK_OPACITY: f32 = 0.0;
fn display_smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}
fn display_grade(input: vec3<f32>, uv: vec2<f32>) -> vec3<f32> {
    var rgb = input;
    if POWER_R != 1.0 { rgb.r = pow(rgb.r, POWER_R); }
    if POWER_G != 1.0 { rgb.g = pow(rgb.g, POWER_G); }
    if POWER_B != 1.0 { rgb.b = pow(rgb.b, POWER_B); }
    if VIGNETTE {
        let r = length((uv - vec2<f32>(0.5)) * vec2<f32>(AXIS_X, AXIS_Y)) / sqrt(0.5);
        rgb = mix(rgb, vec3<f32>(VG_TINT_R, VG_TINT_G, VG_TINT_B), display_smooth(VG_VG_TINT_BEGIN, VG_TINT_END, r) * VG_TINT_A);
        rgb *= mix(1.0, display_smooth(DARK_BEGIN, DARK_END, r), DARK_OPACITY);
    }
    return rgb;
}
fn positive_hdr(v: f32) -> f32 {
    let bits = bitcast<u32>(v);
    if (bits & 0x80000000u) != 0u || (bits & 0x7fffffffu) > 0x7f800000u { return 0.0; }
    return v;
}
