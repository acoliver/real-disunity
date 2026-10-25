@group(0) @binding(1) var source: texture_2d<f32>;
@fragment fn fs_screen(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let base = vec2<i32>(position.xy) * 2;
    var sum = vec4<f32>(0.0);
    var count = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            // Replication gives a defined footprint even for one-pixel axes.
            let value = textureLoad(source, min(base + vec2<i32>(x, y), size - vec2<i32>(1)), 0);
            if value.w > 0.0 {
                sum += value;
                count += 1.0;
            }
        }
    }
    if count == 0.0 { return vec4<f32>(0.0); }
    // The average vector retains directional coherence, rather than inventing
    // a unit normal when opposite-facing surfaces share a footprint.
    return sum / count;
}
