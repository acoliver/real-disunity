@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var lower: texture_2d<f32>;
struct Exposure { value: f32, initialized: u32, pad: vec2<u32> }
@group(0) @binding(2) var<storage, read> exposure: Exposure;
override THRESHOLD: f32 = 1.0;
override KNEE: f32 = 0.5;
override INTENSITY: f32 = 0.0;
override FIXED: f32 = 1.0;
override ADAPTIVE: bool = false;
override FXAA: f32 = 0.75;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn clean(v: f32) -> f32 {
    let b = bitcast<u32>(v);
    if (b & 0x80000000u) != 0u || (b & 0x7f800000u) == 0x7f800000u { return 0.0; }
    return v;
}
fn load(t: texture_2d<f32>, p: vec2<i32>) -> vec4<f32> {
    return textureLoad(t, clamp(p, vec2<i32>(0), vec2<i32>(textureDimensions(t)) - 1), 0);
}
fn linear(t: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    let p = uv * vec2<f32>(textureDimensions(t)) - 0.5;
    let q = vec2<i32>(floor(p)); let f = fract(p);
    return mix(mix(load(t,q), load(t,q+vec2<i32>(1,0)), f.x), mix(load(t,q+vec2<i32>(0,1)), load(t,q+vec2<i32>(1)), f.x), f.y);
}
@fragment fn prefilter(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = load(source, vec2<i32>(p.xy)).rgb;
    let v = vec3<f32>(clean(c.r), clean(c.g), clean(c.b));
    let brightness = max(v.r, max(v.g, v.b));
    let knee = THRESHOLD * KNEE;
    var soft = clamp(brightness - THRESHOLD + knee, 0.0, 2.0 * knee);
    soft = soft * soft / max(4.0 * knee, 0.00001);
    let weight = max(brightness - THRESHOLD, soft) / max(brightness, 0.00001);
    return vec4<f32>(v * weight, 1.0);
}
// Normalized 13-tap downsample. Four overlapping boxes plus the inner box.
@fragment fn downsample(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (p.xy * 2.0) / vec2<f32>(textureDimensions(source));
    let d = 1.0 / vec2<f32>(textureDimensions(source));
    var c = linear(source,uv).rgb * 0.125;
    for (var y = -2; y <= 2; y += 2) {
        for (var x = -2; x <= 2; x += 2) {
            if x == 0 && y == 0 { continue; }
            let w = select(0.0625, 0.03125, x != 0 && y != 0);
            c += linear(source,uv + vec2<f32>(f32(x),f32(y))*d).rgb * w;
        }
    }
    for (var y = -1; y <= 1; y += 2) {
        for (var x = -1; x <= 1; x += 2) {
            c += linear(source,uv + vec2<f32>(f32(x),f32(y))*d).rgb * 0.125;
        }
    }
    return vec4<f32>(c, 1.0);
}
@fragment fn upsample(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = p.xy / vec2<f32>(textureDimensions(source));
    let d = 1.0 / vec2<f32>(textureDimensions(lower));
    var c = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let w = f32(select(1,2,x==0)*select(1,2,y==0)) / 16.0;
            c += linear(lower,uv+vec2<f32>(f32(x),f32(y))*d).rgb*w;
        }
    }
    return vec4<f32>((load(source,vec2<i32>(p.xy)).rgb+c)*0.5,1.0);
}
fn mapped(r: f32, e: f32) -> f32 {
    let b = bitcast<u32>(r);
    if e == 0.0 || (b & 0x80000000u) != 0u || (b & 0x7fffffffu) > 0x7f800000u { return 0.0; }
    if b == 0x7f800000u { return 1.0; }
    let v = r * e;
    return v / (1.0 + v);
}
@fragment fn tone(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = load(source,vec2<i32>(p.xy));
    let bloom = load(lower,vec2<i32>(p.xy)).rgb * INTENSITY;
    let e = select(FIXED, exposure.value, ADAPTIVE);
    // Classify original HDR before adding finite bloom, preserving special-value policy.
    let base = vec3<f32>(mapped(c.r,e),mapped(c.g,e),mapped(c.b,e));
    let v = vec3<f32>(clean(c.r),clean(c.g),clean(c.b)) + bloom;
    var result = vec3<f32>(mapped(v.r,e),mapped(v.g,e),mapped(v.b,e));
    if (bitcast<u32>(c.r) & 0x7f800000u) == 0x7f800000u { result.r = base.r; }
    if (bitcast<u32>(c.g) & 0x7f800000u) == 0x7f800000u { result.g = base.g; }
    if (bitcast<u32>(c.b) & 0x7f800000u) == 0x7f800000u { result.b = base.b; }
    return vec4<f32>(result,c.a);
}
fn luma(v: vec3<f32>) -> f32 { return dot(sqrt(max(v,vec3<f32>(0.0))),vec3<f32>(0.299,0.587,0.114)); }
// Directional FXAA: span 8, reduce 1/8, minimum reduce 1/128, contrast skip 1/32.
@fragment fn fxaa(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(source)); let uv = p.xy/size; let d = 1.0/size;
    let center = load(source,vec2<i32>(p.xy));
    let nw=luma(linear(source,uv+vec2<f32>(-1.0,-1.0)*d).rgb);
    let ne=luma(linear(source,uv+vec2<f32>(1.0,-1.0)*d).rgb);
    let sw=luma(linear(source,uv+vec2<f32>(-1.0,1.0)*d).rgb);
    let se=luma(linear(source,uv+vec2<f32>(1.0,1.0)*d).rgb);
    let m=luma(center.rgb); let lo=min(m,min(min(nw,ne),min(sw,se))); let hi=max(m,max(max(nw,ne),max(sw,se)));
    if hi-lo < max(1.0/32.0,hi/8.0) { return center; }
    var dir=vec2<f32>(-((nw+ne)-(sw+se)),(nw+sw)-(ne+se));
    let reduce=max((nw+ne+sw+se)*(0.25/8.0),1.0/128.0);
    dir=clamp(dir/(min(abs(dir.x),abs(dir.y))+reduce),vec2<f32>(-8.0),vec2<f32>(8.0))*d;
    let a=0.5*(linear(source,uv+dir*(-1.0/6.0)).rgb+linear(source,uv+dir*(1.0/6.0)).rgb);
    let b=a*0.5+0.25*(linear(source,uv+dir*(-0.5)).rgb+linear(source,uv+dir*0.5).rgb);
    let lb=luma(b); let result=select(b,a,lb<lo || lb>hi);
    return vec4<f32>(mix(center.rgb,result,FXAA),center.a);
}
