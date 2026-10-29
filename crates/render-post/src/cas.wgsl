// WGSL port of the sharpen-only path of AMD FidelityFX CAS (contrast adaptive sharpening),
// CasFilter with noScaling, from ffx_cas.h 1.20190610 in
// https://github.com/GPUOpen-Effects/FidelityFX-CAS.
//
//==============================================================================================================================
// LICENSE
// =======
// Copyright (c) 2017-2019 Advanced Micro Devices, Inc. All rights reserved.
// -------
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation
// files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy,
// modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
// -------
// The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
// Software.
// -------
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
// WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//------------------------------------------------------------------------------------------------------------------------------
//
// Differences from the reference: the shader is a fragment pass, not a compute shader, and
// out-of-range taps clamp to the nearest edge texel. It uses the default quality path
// (green-channel filter weights, no CAS_BETTER_DIAGONALS) with full-precision reciprocal and
// square root, as CAS_GO_SLOWER selects, instead of the approximations. The scaling path and
// the packed 16-bit form are not ported. The input is display-referred linear color in 0..1,
// which the reference requires; alpha is copied unchanged.
@group(0) @binding(0) var source: texture_2d<f32>;
// -1 / lerp(8, 5, sharpness) from CasSetup, computed on the CPU.
override PEAK: f32 = -0.125;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn load(p: vec2<i32>) -> vec4<f32> {
    return textureLoad(source, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(source)) - 1), 0);
}
@fragment fn cas(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let q = vec2<i32>(p.xy);
    //  . b .
    //  d e f
    //  . h .
    let b = load(q + vec2<i32>(0, -1)).rgb;
    let d = load(q + vec2<i32>(-1, 0)).rgb;
    let e = load(q);
    let f = load(q + vec2<i32>(1, 0)).rgb;
    let h = load(q + vec2<i32>(0, 1)).rgb;
    // Minimum distance to the signal limit divided by the local maximum, from the green
    // channel, shaped with a square root. 0 means no sharpening and 1 full sharpening.
    let mn = min(min(min(d.g, e.g), f.g), min(b.g, h.g));
    let mx = max(max(max(d.g, e.g), f.g), max(b.g, h.g));
    let amp = sqrt(saturate(min(mn, 1.0 - mx) * (1.0 / max(mx, 1e-20))));
    // Filter shape: a negative lobe in a ring around a centre weight of one.
    let w = amp * PEAK;
    let rcp_weight = 1.0 / (1.0 + 4.0 * w);
    return vec4<f32>(saturate(((b + d + f + h) * w + e.rgb) * rcp_weight), e.a);
}
