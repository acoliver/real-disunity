
@group(0) @binding(0) var normals: texture_2d<f32>;
@group(0) @binding(1) var material: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var environment: texture_cube<f32>;
@group(0) @binding(4) var lut: texture_2d<f32>;
@group(0) @binding(5) var linear_sampler: sampler;
@fragment fn fs_screen(@builtin(position) pixel: vec4<f32>) -> @location(0) f32 {{
    let p = vec2<i32>(pixel.xy);
    let normal = normalize(({} * vec4(textureLoad(normals,p,0).xyz,0.0)).xyz);
    let uv = (vec2<f32>(p) + textureLoad(material,p,0).rg)/32.0;
    let h = {} * vec4(uv * vec2(2.0,-2.0)+vec2(-1.0,1.0),textureLoad(depth,p,0),1.0);
    let view = normalize(vec3({},{},{}) - h.xyz/h.w);
    let nv = clamp(dot(normal,view),0.0001,1.0);
    // Explicit selected coordinate, independent of the forward helper and publication.
    let coefficients = textureSampleLevel(lut,linear_sampler,vec2(nv,0.045),0.0).rg;
    let factor = 0.02037*coefficients.x+coefficients.y;
    let radiance = textureSampleLevel(environment,linear_sampler,reflect(-view,normal),0.045*f32(textureNumLevels(environment)-1u)).r;
    return radiance*factor;
}}