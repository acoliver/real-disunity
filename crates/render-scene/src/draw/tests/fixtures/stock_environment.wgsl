// Metallic-roughness shading: roughness in green, metalness in blue.
// Normal maps encode tangent-space X/Y; the positive Z component is reconstructed.


// Frame state, the reflectance model and the derivative tangent frame arrive
// from lighting.wgsl, which is prepended to this source.

struct Material {
    alpha_cutoff: f32,
    roughness: f32,
    metalness: f32,
    texture_flags: u32,
}

const FLAG_NORMAL: u32 = 1u;
const FLAG_ROUGH_METAL: u32 = 2u;
const FLAG_AMBIENT_OCCLUSION: u32 = 4u;
const FLAG_SELF_ILLUMINATION: u32 = 8u;

@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var albedo_texture: texture_2d<f32>;
@group(1) @binding(2) var normal_texture: texture_2d<f32>;
@group(1) @binding(3) var rough_metal_texture: texture_2d<f32>;
@group(1) @binding(4) var ambient_occlusion_texture: texture_2d<f32>;
@group(1) @binding(5) var self_illumination_texture: texture_2d<f32>;
@group(1) @binding(6) var material_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(8) lod_billboard_mode: u32,
}

struct InstanceInput {
    @location(3) model_column_0: vec4<f32>,
    @location(4) model_column_1: vec4<f32>,
    @location(5) model_column_2: vec4<f32>,
    @location(6) model_column_3: vec4<f32>,
}

// Draw the authored geometry unturned. Crossed quads land here too: they earn
// their approximation by not turning, so there is nothing for this stage to do.
const BILLBOARD_FIXED: u32 = 0u;
// Pivot about the scene's vertical axis only, so the card stays upright.
const BILLBOARD_YAW_LOCKED: u32 = 1u;
// Face the camera on every axis.
const BILLBOARD_SPHERICAL: u32 = 2u;

// Scene space is Z-up: the basis change to Y-up happens at the view level, not
// here. So a billboard pivots about +Z, and this is the axis it stands on.
const SCENE_UP: vec3<f32> = vec3<f32>(0.0, 0.0, 1.0);

// Below this the direction being normalised carries no usable direction.
const BILLBOARD_EPSILON: f32 = 1.0e-6;

// How a billboard card must be authored.
//
// The card lies in its local X–Z plane, standing on local +Z, with its outward
// normal along local −Y. The renderer then maps local +Y to the direction
// pointing away from the camera, which makes (right, away, up) right-handed:
// an observer sees local +X to their right and local +Z upward, exactly the
// frame the card was authored in. Choosing local +Y as the outward normal
// instead would make that triple left-handed, and the card would render
// mirrored — a fault that reads as a texturing mistake rather than a basis
// one, which is why the convention is written down here rather than inferred.
fn billboard_basis(mode: u32, pivot: vec3<f32>) -> mat3x3<f32> {
    let to_camera = frame.camera_position.xyz - pivot;

    if mode == BILLBOARD_YAW_LOCKED {
        let horizontal = vec2<f32>(to_camera.x, to_camera.y);
        // A camera directly overhead determines no yaw at all. The answer has
        // to come from somewhere, and it has to be the same answer every
        // frame: a yaw derived from the vanishing horizontal would swing
        // wildly as the camera crossed the zenith, so hold still instead.
        if dot(horizontal, horizontal) < BILLBOARD_EPSILON {
            return mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), SCENE_UP);
        }
        let away = vec3<f32>(-normalize(horizontal), 0.0);
        return mat3x3<f32>(cross(away, SCENE_UP), away, SCENE_UP);
    }

    let away = -normalize(to_camera);
    let sideways = cross(away, SCENE_UP);
    // Looking straight down the vertical axis leaves the card's roll free; any
    // choice is as correct as another, so take a fixed one to stay stable.
    if dot(sideways, sideways) < BILLBOARD_EPSILON {
        let right = vec3<f32>(1.0, 0.0, 0.0);
        return mat3x3<f32>(right, away, cross(right, away));
    }
    let right = normalize(sideways);
    return mat3x3<f32>(right, away, cross(right, away));
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) texcoord: vec2<f32>,
    @location(3) tint: vec4<f32>,
}

fn transform_vertex(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let model = mat4x4<f32>(
        instance.model_column_0,
        instance.model_column_1,
        instance.model_column_2,
        instance.model_column_3,
    );

    // No normal matrix: SceneTransform carries a uniform scale, so the inverse
    // transpose is this rotation times a positive constant that normalising
    // removes. A non-uniform scale would need the inverse transpose instead.
    var rotation = mat3x3<f32>(
        model[0].xyz,
        model[1].xyz,
        model[2].xyz,
    );
    let pivot = model[3].xyz;

    if vertex.lod_billboard_mode != BILLBOARD_FIXED {
        // The authored rotation is replaced, not composed with: a billboard's
        // whole purpose is to face the camera whatever the placement said.
        // Position and scale survive, and scale is one column's length because
        // SceneTransform scales all three axes alike.
        rotation = billboard_basis(vertex.lod_billboard_mode, pivot) * length(model[0].xyz);
    }

    let world_position = pivot + rotation * vertex.position;

    var out: VertexOutput;
    out.clip_position = frame.view_projection * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.world_normal = normalize(rotation * vertex.normal);
    out.texcoord = vertex.texcoord;
    out.tint = vec4<f32>(1.0);
    return out;
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    return transform_vertex(vertex, instance);
}

struct InstanceTint {
    @location(9) tint: vec4<f32>,
}

@vertex
fn vs_unlit(vertex: VertexInput, instance: InstanceInput, tint: InstanceTint) -> VertexOutput {
    var out = transform_vertex(vertex, instance);
    out.tint = tint.tint;
    return out;
}

// Reconstructs a full normal from the two channels BC5 preserves.
fn sample_normal(uv: vec2<f32>, geometric: vec3<f32>, position: vec3<f32>) -> vec3<f32> {
    if (material.texture_flags & FLAG_NORMAL) == 0u {
        return geometric;
    }
    let xy = textureSample(normal_texture, material_sampler, uv).rg * 2.0 - 1.0;
    let z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
    let frame_basis = cotangent_frame(geometric, position, uv);
    return normalize(frame_basis * vec3<f32>(xy, z));
}

// Shades one fragment. Both entry points funnel here so that the masked and
// opaque pipelines cannot drift apart in anything except the cutout test.
fn shade(input: VertexOutput, front_facing: bool) -> vec4<f32> {
    let albedo_sample = textureSample(albedo_texture, material_sampler, input.texcoord);

    // A double-sided card is one sheet of geometry standing in for a leaf with
    // two faces. Its stored normal describes the front, so the back must be
    // flipped or the leaf lights as though the sun were behind it.
    var geometric_normal = normalize(input.world_normal);
    if !front_facing {
        geometric_normal = -geometric_normal;
    }

    let normal = sample_normal(input.texcoord, geometric_normal, input.world_position);

    var roughness = material.roughness;
    var metalness = material.metalness;
    if (material.texture_flags & FLAG_ROUGH_METAL) != 0u {
        let rough_metal = textureSample(rough_metal_texture, material_sampler, input.texcoord);
        roughness *= rough_metal.g;
        metalness *= rough_metal.b;
    }
    var ambient_occlusion = 1.0;
    if (material.texture_flags & FLAG_AMBIENT_OCCLUSION) != 0u {
        ambient_occlusion = textureSample(
            ambient_occlusion_texture,
            material_sampler,
            input.texcoord,
        ).r;
    }

    var colour = surface_lighting(
        input.world_position,
        normal,
        albedo_sample.rgb,
        roughness,
        metalness,
        ambient_occlusion,
    );

    if (material.texture_flags & FLAG_SELF_ILLUMINATION) != 0u {
        // The mask is a single channel, so it carries intensity and not colour;
        // the emitted hue can only come from the surface's own albedo.
        let emission = textureSample(
            self_illumination_texture,
            material_sampler,
            input.texcoord,
        ).r;
        colour += albedo_sample.rgb * emission;
    }

    // Emission is summed before exposure so that a glowing surface is scaled by
    // the same camera exposure as the sunlight beside it.
    return vec4<f32>(apply_exposure(colour), albedo_sample.a);
}

@fragment
fn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(input, front_facing);
    return vec4<f32>(shaded.rgb, 1.0);
}

fn scene_alpha_covered(alpha: f32) -> bool {
    return alpha >= material.alpha_cutoff;
}

@fragment
fn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let shaded = shade(input, front_facing);
    // Cutout, not blending: the fragment is either present or absent, so no
    // draw ordering is required and the depth buffer stays authoritative.
    if !scene_alpha_covered(shaded.a) {
        discard;
    }
    return vec4<f32>(shaded.rgb, 1.0);
}

@fragment
fn fs_unlit(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(albedo_texture, material_sampler, input.texcoord) * input.tint;
}
