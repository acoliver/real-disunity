// WGSL port of AMD FidelityFX FSR 1 EASU and RCAS (ffx_fsr1.h).
// Uses full-precision reciprocal and square root; texture loads clamp at borders.
// Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files(the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and / or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions :
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.
@group(0) @binding(0) var source: texture_2d<f32>;
override OUTPUT_WIDTH: f32 = 1.0;
override OUTPUT_HEIGHT: f32 = 1.0;
override SHARPNESS: f32 = 0.0;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn load(p: vec2<i32>) -> vec4<f32> {
    return textureLoad(source, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(source)) - 1), 0);
}
fn position(p: vec2<f32>) -> vec2<f32> {
    return p * vec2<f32>(textureDimensions(source)) / vec2<f32>(OUTPUT_WIDTH, OUTPUT_HEIGHT) - 0.5;
}
fn linear(p: vec2<f32>) -> vec4<f32> {
    let q = vec2<i32>(floor(p)); let f = fract(p);
    return mix(mix(load(q), load(q+vec2<i32>(1,0)), f.x), mix(load(q+vec2<i32>(0,1)), load(q+vec2<i32>(1,1)), f.x), f.y);
}
@fragment fn bilinear(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return linear(position(p.xy));
}
fn luma(c: vec3<f32>) -> f32 { return c.b * 0.5 + (c.r * 0.5 + c.g); }
fn edge(q: vec2<i32>, weight: f32) -> vec3<f32> {
    let a = luma(load(q+vec2<i32>(0,-1)).rgb);
    let b = luma(load(q+vec2<i32>(-1,0)).rgb);
    let c = luma(load(q).rgb);
    let d = luma(load(q+vec2<i32>(1,0)).rgb);
    let e = luma(load(q+vec2<i32>(0,1)).rgb);
    let direction = vec2<f32>(d-b, e-a);
    let magnitude = max(abs(vec2<f32>(d,e)-c), abs(c-vec2<f32>(b,a)));
    let length = clamp(abs(direction) / max(magnitude, vec2<f32>(1e-20)), vec2<f32>(0.0), vec2<f32>(1.0));
    return vec3<f32>(direction, dot(length,length)) * weight;
}
fn tap(off: vec2<f32>, dir: vec2<f32>, len: vec2<f32>, lob: f32) -> f32 {
    let v = vec2<f32>(dot(off,dir), dot(off,vec2<f32>(-dir.y,dir.x))) * len;
    let d2 = min(dot(v,v), 1.0/lob);
    let base = 0.4*d2-1.0;
    let window = lob*d2-1.0;
    return (1.5625*base*base-0.5625)*window*window;
}
@fragment fn easu(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pp = position(p.xy); let q = vec2<i32>(floor(pp)); let f = fract(pp);
    let gradient = edge(q,(1.0-f.x)*(1.0-f.y))
        + edge(q+vec2<i32>(1,0),f.x*(1.0-f.y))
        + edge(q+vec2<i32>(0,1),(1.0-f.x)*f.y)
        + edge(q+vec2<i32>(1,1),f.x*f.y);
    var dir = vec2<f32>(1.0,0.0);
    if dot(gradient.xy,gradient.xy) >= 1.0/32768.0 { dir = normalize(gradient.xy); }
    let len = (gradient.z*0.5)*(gradient.z*0.5);
    let stretch = dot(dir,dir)/max(abs(dir.x),abs(dir.y));
    let len2 = vec2<f32>(1.0+(stretch-1.0)*len,1.0-0.5*len);
    let lob = 0.5 + (0.25-0.04-0.5)*len;
    let offsets = array<vec2<i32>,12>(
        vec2<i32>(0,-1),vec2<i32>(1,-1),vec2<i32>(-1,1),vec2<i32>(0,1),
        vec2<i32>(0,0),vec2<i32>(-1,0),vec2<i32>(1,1),vec2<i32>(2,1),
        vec2<i32>(2,0),vec2<i32>(1,0),vec2<i32>(1,2),vec2<i32>(0,2));
    var color = vec3<f32>(0.0); var weight = 0.0;
    for (var i = 0u; i < 12u; i++) {
        let w = tap(vec2<f32>(offsets[i])-f,dir,len2,lob);
        color += load(q+offsets[i]).rgb*w; weight += w;
    }
    let a = load(q).rgb; let b = load(q+vec2<i32>(1,0)).rgb;
    let c = load(q+vec2<i32>(0,1)).rgb; let d = load(q+vec2<i32>(1,1)).rgb;
    return vec4<f32>(clamp(color/weight,min(min(a,b),min(c,d)),max(max(a,b),max(c,d))),linear(pp).a);
}
@fragment fn rcas(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let q = vec2<i32>(p.xy);
    let b = load(q+vec2<i32>(0,-1)).rgb; let d = load(q+vec2<i32>(-1,0)).rgb;
    let e = load(q); let f = load(q+vec2<i32>(1,0)).rgb; let h = load(q+vec2<i32>(0,1)).rgb;
    let mn = min(min(b,d),min(f,h)); let mx = max(max(b,d),max(f,h));
    // Degenerate black/white rings have zero permitted negative lobe.
    let hit_min = min(mn,e.rgb)/max(4.0*mx,vec3<f32>(1e-20));
    let hit_max = (1.0-max(mx,e.rgb))/min(4.0*mn-4.0,vec3<f32>(-1e-20));
    let limit = max(-hit_min,hit_max);
    let lob = max(-0.1875,min(max(limit.r,max(limit.g,limit.b)),0.0))*exp2(-SHARPNESS);
    return vec4<f32>((lob*(b+d+h+f)+e.rgb)/(4.0*lob+1.0),e.a);
}
