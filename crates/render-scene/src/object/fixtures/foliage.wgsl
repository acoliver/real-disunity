// The authored force basis is deliberately not orthogonalized.
// Curvature, rather than radius, represents the straight limit without infinity.
fn scene_arc(sample: vec3<f32>, force: vec3<f32>, curvature: f32) -> vec4<f32> {
    return scene_arc_policy(sample, force, curvature, false);
}
fn scene_arc_policy(sample: vec3<f32>, force: vec3<f32>, curvature: f32, minimum_arc: bool) -> vec4<f32> {
    let distance = length(sample);
    if distance == 0.0 { return vec4<f32>(0.0); }
    if curvature == 0.0 || (length(force) == 0.0 && !minimum_arc) { return vec4<f32>(sample, 0.0); }
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
    return scene_arc_curvature_policy(force, extent, false);
}
fn scene_arc_curvature_policy(force: vec3<f32>, extent: f32, minimum_arc: bool) -> f32 {
    let angle = 2.0 * atan(length(force) / extent);
    if minimum_arc { return max(PI / 360.0, angle) / extent; }
    return angle / extent;
}

fn scene_band(seed: vec3<f32>, band: vec4<u32>, time: u32, phase: f32) -> f32 {
    let spatial = dot(seed, vec3<f32>(bitcast<f32>(band.z)));
    let wrapped = spatial - floor(spatial / (2.0 * PI)) * (2.0 * PI);
    let frequency = bitcast<f32>(band.w);
    let period = u32(2.0 * PI / frequency * 1000.0);
    let angle = f32(time % period) / 1000.0 * frequency;
    // WGSL only bounds sin accuracy on [-pi, pi]; reduce the complete phase.
    let combined = angle + wrapped + phase;
    let reduced = combined - floor((combined + PI) / (2.0 * PI)) * (2.0 * PI);
    return 1.0 + sin(clamp(reduced, -PI, PI)) * bitcast<f32>(band.y);
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
    return scene_curved_position_forces(input,input.force*input.radial_factor);
}
fn scene_curved_position_forces(input: SceneBendInput, radial_force: vec3<f32>) -> vec3<f32> {
    return scene_curved_position_policy(input, radial_force, false);
}
fn scene_curved_position_policy(input: SceneBendInput, radial_force: vec3<f32>, minimum_arc: bool) -> vec3<f32> {
    if !minimum_arc && all(input.force == vec3<f32>(0.0)) && all(radial_force == vec3<f32>(0.0)) { return input.position; }
    let height = min(dot(input.position, input.axis), input.extents.x);
    let attachment = input.position - input.axis * height;
    let curvature = min(2.0, scene_arc_curvature_policy(input.force, input.extents.x, minimum_arc));
    let weight = pow(clamp(abs(height) / input.extents.x, 0.0, 1.0), input.curve - 1.0);
    let bent = scene_arc_policy(height * input.axis, input.force, curvature * max(weight, 0.0000001), minimum_arc);
    var rotated = attachment;
    let hinge = cross(input.force, input.axis);
    // Parallel force has no rotation axis. Zero-length vectors have no tangent.
    if length(input.force) > 0.001 && dot(hinge, hinge) > 0.0 {
        let unit = normalize(hinge);
        let angle = -bent.w;
        rotated = attachment * cos(angle) + cross(unit, attachment) * sin(angle)
            + unit * dot(unit, attachment) * (1.0 - cos(angle));
    }
    let detail_force = radial_force;
    let detail = scene_arc_policy(rotated, detail_force, scene_arc_curvature_policy(detail_force, input.extents.y, minimum_arc), minimum_arc);
    let blend = clamp((height / input.extents.x - 0.3) * 8.0, 0.0, 1.0);
    return bent.xyz + mix(rotated, detail.xyz, blend);
}

fn scene_geometry_displacement(position: vec3<f32>, vertex_seed: f32, instance: InstanceInput) -> vec3<f32> {
    return scene_geometry_displacement_at(position, vertex_seed, instance, bitcast<u32>(frame.grading.z));
}

// The deformation is a pure function of the clock, so the previous frame's shape is this same
// evaluation at the previous frame's clock.
fn scene_geometry_displacement_at(position: vec3<f32>, vertex_seed: f32, instance: InstanceInput, time: u32) -> vec3<f32> {
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
    var base_force = material.wind_direction_amplitude.xyz * material.wind_direction_amplitude.w;
    var modifiers = vec3<f32>(0.0);
    if material.wind_field_mode.x != 0.0 {
        let uv = (position.xy-material.wind_field_domain.xy)/material.wind_field_domain.zw;
        let inset = 0.5 / vec2<f32>(textureDimensions(wind_force_field));
        base_force = vec3<f32>(textureSampleLevel(wind_force_field,material_sampler,clamp(uv,inset,1.0-inset),0.0).rg*2.0-1.0,0.0) * material.wind_direction_amplitude.w;
        for (var i=0u; i<3u; i++) {
            let shape = material.wind_modifiers[i*2u+1u];
            if shape.z != 0.0 { modifiers += scene_static_modifier(position,material.wind_modifiers[i*2u],shape); }
        }
        modifiers *= settings.z;
    }
    let force = base_force * gain + modifiers;
    let detail = scene_band(seed, material.foliage_radial, time, phase) * settings.z;
    let relative = position - origin;
    let input = SceneBendInput(relative, axis, force, settings.xy, material.foliage_axis_curve.w, detail);
    let minimum_arc = material.foliage_origin.w != 0.0;
    if material.wind_field_mode.x == 0.0 { return scene_curved_position_policy(input, input.force * input.radial_factor, minimum_arc) - relative; }
    return scene_curved_position_policy(input, base_force * gain * detail + modifiers, minimum_arc) - relative;
}

fn scene_static_modifier(position: vec3<f32>, center_height: vec4<f32>, shape: vec4<f32>) -> vec3<f32> {
    var delta = position-center_height.xyz;
    delta.z -= clamp(delta.z,-center_height.w,center_height.w);
    let distance = length(delta);
    var direction = vec3<f32>(1.0,0.0,0.0);
    if distance >= 0.00001 { direction = delta/distance; }
    return direction * shape.z * smoothstep(0.0,1.0,1.0-(distance-shape.x)/(shape.y-shape.x));
}

// Written by the host for every frame. Pipelines that do not name it ignore it.
// camera_time holds the previous camera position and, as raw u32 bits, the previous clock.
struct PreviousFrame {
    view_projection: mat4x4<f32>,
    camera_time: vec4<f32>,
}

@group(0) @binding(6) var<uniform> previous_frame: PreviousFrame;

// The deformed world position of an undeformed one at a given clock. Current and previous
// positions both come from this one function so that equal clocks yield identical bits.
fn scene_deformed_position_at(position: vec3<f32>, vertex_seed: f32, instance: InstanceInput, time: u32) -> vec3<f32> {
    if material.wind_clock.x == 0u { return position; }
    return position + scene_geometry_displacement_at(position, vertex_seed, instance, time);
}

// World-space motion of a vertex since the previous frame, `previous - current`. `position` is
// the undeformed current world position and `before_position` the undeformed world position
// under `before_instance`, the instance with the previous frame's transform. Both terms come
// from `scene_deformed_position_at`, so equal transforms and clocks yield exactly zero.
fn scene_world_delta(position: vec3<f32>, before_position: vec3<f32>, vertex_seed: f32, instance: InstanceInput, before_instance: InstanceInput) -> vec3<f32> {
    let now = bitcast<u32>(frame.grading.z);
    let before = bitcast<u32>(previous_frame.camera_time.w);
    return scene_deformed_position_at(before_position, vertex_seed, before_instance, before)
        - scene_deformed_position_at(position, vertex_seed, instance, now);
}

// The same for an instance that did not move, which is every instance without a tracked
// previous transform.
fn scene_wind_world_delta(position: vec3<f32>, vertex_seed: f32, instance: InstanceInput) -> vec3<f32> {
    return scene_world_delta(position, position, vertex_seed, instance, instance);
}
