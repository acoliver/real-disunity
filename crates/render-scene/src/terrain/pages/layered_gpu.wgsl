struct Layer {
    textures: vec4<u32>,
    mapping: vec4<f32>,
    tint_radius: vec4<f32>,
    rotation_offsets: vec4<f32>,
}
struct Image { shape: vec4<u32> }
struct Source {
    grid: vec4<u32>,
    mapping: vec4<f32>,
    colorization: vec4<u32>,
    color_mapping: vec4<f32>,
    layers: array<Layer>,
}
@group(0) @binding(0) var<storage, read> layered_source: Source;
@group(0) @binding(1) var<storage, read> layered_texels: array<u32>;
@group(0) @binding(2) var<storage, read> layered_images: array<Image>;

fn layered_read(image: Image, at: vec2<i32>, repeat: bool) -> vec4<f32> {
    let size = vec2<i32>(image.shape.xy);
    var p = clamp(at, vec2<i32>(0), size - 1);
    if repeat { p = ((at % size) + size) % size; }
    let v = layered_texels[image.shape.z + u32(p.y) * image.shape.x + u32(p.x)];
    var color = vec4<f32>(f32(v & 255u), f32((v >> 8u) & 255u), f32((v >> 16u) & 255u), f32(v >> 24u)) / 255.0;
    if (image.shape.w & 1u) != 0u {
        color = vec4<f32>(select(pow((color.rgb + 0.055) / 1.055, vec3<f32>(2.4)), color.rgb / 12.92, color.rgb <= vec3<f32>(0.04045)), color.a);
    }
    return color;
}
fn layered_level(index: u32, level: u32) -> Image {
    var image = layered_images[index];
    for (var mip = 0u; mip < level; mip++) {
        image.shape.z += image.shape.x * image.shape.y;
        image.shape = vec4<u32>(max(image.shape.xy / 2u, vec2<u32>(1u)), image.shape.zw);
    }
    return image;
}
fn layered_bilinear(image: Image, uv: vec2<f32>, repeat: bool) -> vec4<f32> {
    var p = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    if repeat { p = fract(uv); }
    p = p * vec2<f32>(image.shape.xy) - 0.5;
    let base = vec2<i32>(floor(p));
    let f = fract(p);
    let a = layered_read(image, base, repeat);
    let b = layered_read(image, base + vec2<i32>(1, 0), repeat);
    let c = layered_read(image, base + vec2<i32>(0, 1), repeat);
    let d = layered_read(image, base + vec2<i32>(1), repeat);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
fn layered_texture(index: u32, uv: vec2<f32>, repeat: bool) -> vec4<f32> {
    return layered_bilinear(layered_images[index], uv, repeat);
}
fn layered_trilinear(index: u32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let image = layered_images[index];
    let maximum = image.shape.w >> 1u;
    if maximum == 0u { return layered_bilinear(image, uv, true); }
    let size = vec2<f32>(image.shape.xy);
    let footprint = max(length(dx * size), length(dy * size));
    let lod = clamp(log2(max(footprint, 1.0)), 0.0, f32(maximum));
    let lower = u32(floor(lod));
    let upper = min(lower + 1u, maximum);
    let a = layered_bilinear(layered_level(index, lower), uv, true);
    let b = layered_bilinear(layered_level(index, upper), uv, true);
    return mix(a, b, fract(lod));
}
fn layered_rotate(value: vec2<f32>, layer: Layer) -> vec2<f32> {
    return vec2<f32>(value.x * layer.rotation_offsets.x - value.y * layer.rotation_offsets.y,
        value.x * layer.rotation_offsets.y + value.y * layer.rotation_offsets.x) / layer.mapping.z;
}
fn layered_member_at(layer: u32, at: vec2<i32>) -> f32 {
    let p = vec2<u32>(clamp(at, vec2<i32>(0), vec2<i32>(layered_source.grid.xy) - 1));
    return select(0.0, 1.0, layered_texels[layered_source.grid.z + p.y * layered_source.grid.x + p.x] == layer);
}
fn layered_membership(layer: u32, grid: vec2<f32>) -> f32 {
    let p = clamp(grid - 0.5, vec2<f32>(-1.0), vec2<f32>(layered_source.grid.xy));
    let b = vec2<i32>(floor(p));
    let f = fract(p);
    return mix(mix(layered_member_at(layer, b), layered_member_at(layer, b + vec2<i32>(1, 0)), f.x),
        mix(layered_member_at(layer, b + vec2<i32>(0, 1)), layered_member_at(layer, b + vec2<i32>(1)), f.x), f.y);
}
const LAYERED_OFFSETS = array<vec2<f32>, 14>(
    vec2<f32>(0.0), vec2<f32>(0.023591, 0.067420), vec2<f32>(-0.111690, 0.089070),
    vec2<f32>(-0.181441, -0.114006), vec2<f32>(0.063576, -0.278551), vec2<f32>(0.354897, -0.039989),
    vec2<f32>(0.185952, 0.386129), vec2<f32>(-0.353551, 0.353556), vec2<f32>(-0.514841, -0.247930),
    vec2<f32>(0.071972, -0.638816), vec2<f32>(0.696376, -0.158950), vec2<f32>(0.418032, 0.665279),
    vec2<f32>(-0.534412, 0.670147), vec2<f32>(-0.876467, -0.306677));
const LAYERED_WEIGHTS = array<f32, 14>(1.0, 0.9285717, 0.857143, 0.7857146, 0.7142858, 0.6428571,
    0.5714282, 0.4999998, 0.4285714, 0.3571424, 0.2857134, 0.2142851, 0.1428577, 0.0714288);
fn layered_coverage(layer: u32, world: vec2<f32>, radius: f32) -> f32 {
    let grid = (world - layered_source.mapping.xy) / layered_source.mapping.z;
    if layered_source.grid.w == 0u { return layered_membership(layer, grid); }
    let scaled = world * 1000000.0;
    let h = select(vec2<u32>(clamp(scaled, vec2<f32>(0.0), vec2<f32>(4294967040.0))), vec2<u32>(0xffffffffu), scaled >= vec2<f32>(4294967296.0));
    let hash = ((h.x * 1597334677u) ^ (h.y * 3812015801u)) * 1597334677u;
    let noise = f32(hash) / 4294967295.0;
    let sector = u32(noise * 100000.0) % 18u;
    let angle = f32(sector) * 3.141592653589793 / 9.0;
    let s = round(sin(angle) * 1000000.0) / 1000000.0;
    let c = round(cos(angle) * 1000000.0) / 1000000.0;
    let r = radius / layered_source.mapping.z * (1.0 + noise * 0.125);
    var coverage = 0.0;
    for (var tap = 0u; tap < 14u; tap++) {
        let o = LAYERED_OFFSETS[tap];
        coverage += layered_membership(layer, grid + vec2<f32>(o.x * s - o.y * c, o.x * c + o.y * s) * r) * LAYERED_WEIGHTS[tap];
    }
    return coverage / 7.499999;
}
struct LayeredResult {
    albedo: vec3<f32>, metallic: f32,
    normal: vec3<f32>, roughness: f32,
    height: f32,
}
fn compose_layered(world: vec2<f32>, world_dx: vec2<f32>, world_dy: vec2<f32>) -> LayeredResult {
    var result = LayeredResult(vec3<f32>(0.0), 0.0, vec3<f32>(0.0), 0.0, 0.0);
    var total = 0.0;
    let spiral = layered_source.grid.w != 0u;
    for (var index = 0u; index < arrayLength(&layered_source.layers); index++) {
        let layer = layered_source.layers[index];
        let coverage = layered_coverage(index, world, layer.tint_radius.w);
        var strength = coverage * select(0.75, 1.0, spiral);
        if strength <= 0.0001 { continue; }
        let rotated = vec2<f32>(world.x * layer.rotation_offsets.x - world.y * layer.rotation_offsets.y,
            world.x * layer.rotation_offsets.y + world.y * layer.rotation_offsets.x);
        let uv = (rotated + layer.mapping.xy) / layer.mapping.z;
        let dx = layered_rotate(world_dx, layer);
        let dy = layered_rotate(world_dy, layer);
        let color = layered_trilinear(layer.textures.x, uv, dx, dy);
        let packed = layered_trilinear(layer.textures.y, uv, dx, dy);
        let height = layered_trilinear(layer.textures.z, uv, dx, dy).r;
        var normal = packed.xyz * 2.0 - 1.0;
        if layer.textures.w != 0u { normal.z = sqrt(max(0.0, 1.0 - dot(normal.xy, normal.xy))); }
        if !spiral { strength *= max(height, 0.01); }
        let share = (1.0 / (strength + total)) * strength;
        let previous = 1.0 - share + select(0.0, result.height, spiral);
        let current = share + select(0.0, height, spiral);
        let threshold = max(previous, current) - 0.5;
        let a = max(previous - threshold, 0.0);
        let b = max(current - threshold, 0.0);
        let fraction = b / (a + b);
        let retain = a / (a + b);
        result.albedo = result.albedo * retain + color.rgb * layer.tint_radius.rgb * fraction;
        result.normal = result.normal * select(retain, 1.0, spiral) + normal * fraction;
        result.roughness = result.roughness * retain + (packed.a + layer.rotation_offsets.z) * fraction;
        result.metallic = result.metallic * retain + (color.a + layer.rotation_offsets.w) * fraction;
        total += strength + select(0.0, height * share, spiral);
        if spiral { result.height = max(result.height, height * strength); }
    }
    result.normal = normalize(result.normal);
    if layered_source.colorization.x != 0u {
        let uv = (world - layered_source.color_mapping.xy) / layered_source.color_mapping.zw;
        let color = layered_texture(layered_source.colorization.y, uv, false);
        var tint = color.rgb;
        if layered_source.colorization.z == 1u { tint = vec3<f32>(color.r); }
        if layered_source.colorization.z == 2u {
            tint = color.bgr;
            result.roughness += (color.a - 0.5) * 2.0;
        }
        result.albedo *= tint * (255.0 / 200.0);
    }
    return result;
}
