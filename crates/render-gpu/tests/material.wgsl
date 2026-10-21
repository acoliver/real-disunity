@group(0) @binding(0) var albedo: texture_2d<f32>;
@group(0) @binding(1) var normal: texture_2d<f32>;
@group(0) @binding(2) var filtering: sampler;
@group(0) @binding(3) var<uniform> material: vec4<u32>;
@group(0) @binding(4) var ao: texture_2d<f32>;
@group(0) @binding(5) var illumination: texture_2d<f32>;
@group(0) @binding(6) var roughness: texture_2d<f32>;
@group(1) @binding(0) var<storage, read_write> output: array<u32>;
@compute @workgroup_size(1)
fn main() {
    output[0] = material.x + u32(round(textureSampleLevel(albedo, filtering, vec2(0.5), 0.0).r * 255.0));
    output[1] = material.x + u32(round(textureSampleLevel(normal, filtering, vec2(0.5), 0.0).r * 255.0));
    output[2] = material.x + u32(round(textureSampleLevel(ao, filtering, vec2(0.5), 0.0).r * 255.0));
    output[3] = material.x + u32(round(textureSampleLevel(illumination, filtering, vec2(0.5), 0.0).r * 255.0));
    output[4] = material.x + u32(round(textureSampleLevel(roughness, filtering, vec2(0.5), 0.0).r * 255.0));
}
