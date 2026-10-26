// The authored force basis is deliberately not orthogonalized.
// Curvature, rather than radius, represents the straight limit without infinity.
fn scene_arc(sample: vec3<f32>, force: vec3<f32>, curvature: f32) -> vec4<f32> {
    let distance = length(sample);
    if distance == 0.0 { return vec4<f32>(0.0); }
    if curvature == 0.0 || length(force) == 0.0 { return vec4<f32>(sample, 0.0); }
    let angle = min(distance * curvature, 0.5 * PI);
    var lateral: f32;
    var axial: f32;
    if angle < 0.01 {
        // Taylor integrals avoid cancellation in 1-cos and division by tiny curvature.
        let squared = angle * angle;
        lateral = distance * angle * (0.5 - squared / 24.0 + squared * squared / 720.0);
        axial = distance * (1.0 - squared / 6.0 + squared * squared / 120.0);
    } else {
        lateral = (1.0 - cos(angle)) / curvature + max(0.0, distance - angle / curvature);
        axial = sin(angle) / curvature;
    }
    return vec4<f32>(force / max(0.0001, length(force)) * lateral + sample / distance * axial, angle);
}

fn scene_arc_curvature(force: vec3<f32>, extent: f32) -> f32 {
    return 2.0 * atan(length(force) / extent) / extent;
}

fn scene_band(seed: vec3<f32>, band: vec4<u32>, time: u32, phase: f32) -> f32 {
    let spatial = dot(seed, vec3<f32>(bitcast<f32>(band.z)));
    let wrapped = spatial - floor(spatial / (2.0 * PI)) * (2.0 * PI);
    let frequency = bitcast<f32>(band.w);
    let period = u32(2.0 * PI / frequency * 1000.0);
    let angle = f32(time % period) / 1000.0 * frequency;
    return 1.0 + sin(angle + wrapped + phase) * bitcast<f32>(band.y);
}

struct SceneBendInput {
    position: vec3<f32>,
    axis: vec3<f32>,
    force: vec3<f32>,
    extents: vec2<f32>,
    curve: f32,
    radial_factor: f32,
}

fn scene_curved_position(input: SceneBendInput) -> vec3<f32> {
    if all(input.force == vec3<f32>(0.0)) { return input.position; }
    let height = min(dot(input.position, input.axis), input.extents.x);
    let attachment = input.position - input.axis * height;
    let curvature = min(2.0, scene_arc_curvature(input.force, input.extents.x));
    let weight = pow(clamp(abs(height) / input.extents.x, 0.0, 1.0), input.curve - 1.0);
    let bent = scene_arc(height * input.axis, input.force, curvature * max(weight, 0.0000001));
    var rotated = attachment;
    let hinge = cross(input.force, input.axis);
    // Parallel force has no rotation axis. Zero-length vectors have no tangent.
    if length(input.force) > 0.001 && dot(hinge, hinge) > 0.0 {
        let unit = normalize(hinge);
        let angle = -bent.w;
        rotated = attachment * cos(angle) + cross(unit, attachment) * sin(angle)
            + unit * dot(unit, attachment) * (1.0 - cos(angle));
    }
    let detail_force = input.force * input.radial_factor;
    let detail = scene_arc(rotated, detail_force, scene_arc_curvature(detail_force, input.extents.y));
    let blend = clamp((height / input.extents.x - 0.3) * 8.0, 0.0, 1.0);
    return bent.xyz + mix(rotated, detail.xyz, blend);
}

fn scene_geometry_displacement(position: vec3<f32>, vertex_seed: f32, instance: InstanceInput) -> vec3<f32> {
    let time = bitcast<u32>(frame.grading.z);
    if material.foliage_axis_curve.w == 0.0 {
        return scene_wind_offset(material.wind_direction_amplitude, material.wind_clock.xy, time);
    }
    let linear = mat3x3<f32>(instance.model_column_0.xyz, instance.model_column_1.xyz, instance.model_column_2.xyz);
    let origin = instance.model_column_3.xyz + linear * material.foliage_origin.xyz;
    let axis = normalize(linear * material.foliage_axis_curve.xyz);
    let settings = material.foliage_extent_strength_phase;
    var seed = position;
    if settings.w == 1.0 { seed = instance.model_column_3.xyz; }
    if settings.w == 2.0 { seed = vec3<f32>(vertex_seed); }
    let phase = f32(material.wind_clock.y) / 1000000.0 * (2.0 * PI);
    let gain = scene_band(seed, material.foliage_primary, time, phase) * settings.z;
    let force = material.wind_direction_amplitude.xyz * material.wind_direction_amplitude.w * gain;
    let detail = scene_band(seed, material.foliage_radial, time, phase) * settings.z;
    let relative = position - origin;
    let input = SceneBendInput(relative, axis, force, settings.xy, material.foliage_axis_curve.w, detail);
    return scene_curved_position(input) - relative;
}
