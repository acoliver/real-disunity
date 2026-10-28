const logical_sizes = array<vec2<i32>, LEVEL_COUNT>(LOGICAL_SIZES);
fn reflected(level: u32, uv: vec2<f32>, sample: u32) -> vec4<f32> {
    let size = logical_sizes[level];
    let coordinates = reflection_coordinates(uv, size, exp2(f32(level)));
    let offset = vec2<i32>(i32(sample % LANE_COLUMNS), i32(sample / LANE_COLUMNS)) * size;
    let a = vec2<i32>(coordinates[0]) + offset;
    let b = vec2<i32>(coordinates[1]) + offset;
    let weight = coordinates[2];
    let lod = i32(level);
    return mix(
        mix(textureLoad(reflection_0, a, lod), textureLoad(reflection_0, vec2<i32>(b.x, a.y), lod), weight.x),
        mix(textureLoad(reflection_0, vec2<i32>(a.x, b.y), lod), textureLoad(reflection_0, b, lod), weight.x), weight.y);
}
