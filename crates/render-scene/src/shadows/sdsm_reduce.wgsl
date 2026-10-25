struct Statistics {
    minimum: atomic<u32>,
    maximum: atomic<u32>,
    samples: atomic<u32>,
    invalid: atomic<u32>,
}
@group(0) @binding(0) var<storage, read_write> statistics: Statistics;
@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var coverage: texture_2d<f32>;

@compute @workgroup_size(1)
fn initialize() {
    atomicStore(&statistics.minimum, 0x7f7fffffu);
    atomicStore(&statistics.maximum, 0u);
    atomicStore(&statistics.samples, 0u);
    atomicStore(&statistics.invalid, 0u);
}

@compute @workgroup_size(8, 8)
fn reduce(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let size = textureDimensions(depths);
    if any(invocation.xy >= size) { return; }
    let position = vec2<i32>(invocation.xy);
    let normal = textureLoad(coverage, position, 0);
    if normal.a == 0.0 { return; }
    let depth = textureLoad(depths, position, 0).r;
    // Ordered positive IEEE floats permit integer atomics. Reject NaN, infinity
    // and nonpositive covered values rather than treating corruption as sky.
    let bits = bitcast<u32>(depth);
    if !(depth > 0.0) || bits >= 0x7f800000u {
        atomicStore(&statistics.invalid, 1u);
        return;
    }
    atomicMin(&statistics.minimum, bits);
    atomicMax(&statistics.maximum, bits);
    atomicAdd(&statistics.samples, 1u);
}
