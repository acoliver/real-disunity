override layered_products: bool = false;
@group(0) @binding(0) var source: texture_2d_array<u32>;
@group(0) @binding(1) var destination: texture_storage_2d_array<rgba8uint, write>;

fn overlap(cell: u32, scale: u32, lo: u32, hi: u32) -> u32 {
    return min((cell + 1u) * scale, hi) - max(cell * scale, lo);
}
@compute @workgroup_size(8, 8)
fn reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(destination);
    if any(id.xy >= size) { return; }
    let old = textureDimensions(source);
    let lo = id.xy * old;
    let hi = (id.xy + vec2<u32>(1u)) * old;
    let first = lo / size;
    let last = (hi + size - vec2<u32>(1u)) / size;
    var color = vec4<u32>(0u);
    var roughness = 0u;
    var height = 0u;
    var normal = vec3<f32>(0.0);
    var first_normal = vec3<f32>(0.0);
    for (var y = first.y; y < last.y; y += 1u) {
        for (var x = first.x; x < last.x; x += 1u) {
            let weight = overlap(x, size.x, lo.x, hi.x) * overlap(y, size.y, lo.y, hi.y);
            let c = textureLoad(source, vec2<u32>(x,y), 0, 0);
            let n = textureLoad(source, vec2<u32>(x,y), 1, 0);
            color += c * weight;
            roughness += n.w * weight;
            if layered_products { height += textureLoad(source, vec2<u32>(x,y), 2, 0).x * weight; }
            let direction = normalize(vec3<f32>(vec3<i32>(n.xyz) * 2 - vec3<i32>(255)));
            if x == first.x && y == first.y { first_normal = direction; }
            normal += direction * f32(weight);
        }
    }
    let total = old.x * old.y;
    if layered_products { textureStore(destination, id.xy, 2, vec4<u32>((height + total / 2u) / total, 0u, 0u, 0u)); }
    // Exact antipodal ties preserve the first row-major source direction.
    var direction = first_normal;
    if dot(normal, normal) > 0.0 { direction = normalize(normal); }
    let encoded = vec3<u32>(floor(clamp((direction * 0.5 + 0.5) * 255.0,
        vec3<f32>(0.0), vec3<f32>(255.0)) + 0.5));
    textureStore(destination, id.xy, 0, (color + vec4<u32>(total / 2u)) / total);
    textureStore(destination, id.xy, 1, vec4<u32>(encoded, (roughness + total / 2u) / total));
}
