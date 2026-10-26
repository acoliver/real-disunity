override layered_products: bool = false;
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var destination: texture_storage_2d_array<rgba8uint, write>;
fn unpack(value: u32) -> vec4<u32> {
    return vec4<u32>(value & 255u, (value >> 8u) & 255u, (value >> 16u) & 255u, value >> 24u);
}
@compute @workgroup_size(8, 8)
fn store(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(destination);
    if any(id.xy >= size) { return; }
    let index = (id.y * size.x + id.x) * select(2u, 4u, layered_products);
    let product = vec2<u32>(source[index], source[index + 1u]);
    textureStore(destination, id.xy, 0, unpack(product.x));
    textureStore(destination, id.xy, 1, unpack(product.y));
    if layered_products { textureStore(destination, id.xy, 2, vec4<u32>(source[index + 2u], 0u, 0u, 0u)); }
}
