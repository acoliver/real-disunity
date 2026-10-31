// Thin feature candidates of the temporal stage at the internal extent R.
//
// A texel is a candidate when its luminance stands out from both neighbours of one of the four
// axes through it (horizontal, vertical and the two diagonals) and those two neighbours agree: a
// wire or a line one texel wide, bright or dark. A step edge has a neighbour equal to the centre
// (contrast 0), and a line wider than one texel has one too. A ramp of slope s has neighbours 2s
// apart and contrast s, which fails the agreement test; the centre of a ramp cannot lie outside
// both neighbours, so no separate sign test is needed. The luminance is the reversible compression y / (1 + y)
// of the Rec. 709 weighted sum, so a bright wire and a dim one are compared on one scale.
//
// `CONTRAST` is the smallest step of the compressed luminance that counts, 1/16: the compressed
// value spans [0, 1), and a step below a sixteenth is within the noise of the half precision
// colour plus the jitter modulation of a flat area. `AGREEMENT` is the largest difference of the
// two neighbours relative to the smaller contrast, 1/4: a wire on a gradient of slope s has
// neighbours 2s apart and qualifies while 2s stays under a quarter of its contrast.
//
// The pass writes `rg32float`: x the candidate strength (the smaller of the two contrasts of the
// best axis, 0 for no candidate) and y the compressed luminance of the texel. The one texel border
// has no complete neighbourhood and writes 0. A nonfinite colour counts as 0 and a luminance
// above the largest half precision value is clamped, so nothing writes NaN.
const CONTRAST: f32 = 0.0625;
const AGREEMENT: f32 = 0.25;
@group(0) @binding(0) var color: texture_2d<f32>;
@group(0) @binding(1) var candidates: texture_storage_2d<rg32float, write>;

fn luma(p: vec2<i32>) -> f32 {
    let c = textureLoad(color, p, 0).rgb;
    let raw = dot(select(vec3<f32>(0.0), max(c, vec3<f32>(0.0)), c == c), vec3<f32>(0.2126, 0.7152, 0.0722));
    let y = min(raw, 65504.0);
    return y / (1.0 + y);
}

// The smaller contrast when the centre `c` lies outside both neighbours and they agree, else 0.
fn axis(c: f32, a: f32, b: f32) -> f32 {
    let da = c - a;
    let db = c - b;
    let low = min(abs(da), abs(db));
    if low < CONTRAST || abs(a - b) > AGREEMENT * low {
        return 0.0;
    }
    return low;
}

@compute @workgroup_size(8, 8, 1)
fn create_locks(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(color);
    if gid.x >= size.x || gid.y >= size.y {
        return;
    }
    let p = vec2<i32>(gid.xy);
    let last = vec2<i32>(size) - vec2<i32>(1);
    if p.x == 0 || p.y == 0 || p.x == last.x || p.y == last.y {
        textureStore(candidates, p, vec4<f32>(0.0));
        return;
    }
    let c = luma(p);
    var best = axis(c, luma(p - vec2<i32>(1, 0)), luma(p + vec2<i32>(1, 0)));
    best = max(best, axis(c, luma(p - vec2<i32>(0, 1)), luma(p + vec2<i32>(0, 1))));
    best = max(best, axis(c, luma(p - vec2<i32>(1, 1)), luma(p + vec2<i32>(1, 1))));
    best = max(best, axis(c, luma(p + vec2<i32>(1, -1)), luma(p + vec2<i32>(-1, 1))));
    textureStore(candidates, p, vec4<f32>(best, c, 0.0, 0.0));
}
