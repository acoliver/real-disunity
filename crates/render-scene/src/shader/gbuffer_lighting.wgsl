// Deferred lighting of the compact G-buffer, one full-screen pass per frame.
//
// This module is appended to the frame's own forward object source together with
// gbuffer.wgsl, so the sun, cascaded shadows, disk highlight, SH or hemisphere ambient,
// screen and material AO, environment and BRDF factor, back-scattering, transmission,
// emission and fog are the forward functions with the forward variant's specialisation. It
// reads only set 0 (frame), set 1 bindings 16 and up (below), set 2 (shadows) and set 3
// (screen-space inputs), never the material at set 1.
//
// A pixel whose G3 is zero was written by no producer and is left zero. Every lighting
// function runs for every pixel before that choice, so the shadow lookups' derivatives stay
// in uniform control flow.

struct SceneDeferredCamera {
    // Inverse of frame.view_projection for homogeneous clip depth z - w.
    scene_from_clip: mat4x4<f32>,
    // Scene direction from view direction, the transpose of the producers' view_from_scene.
    scene_from_view: mat3x3<f32>,
    receiver_policy: vec4<u32>,
}

@group(1) @binding(16) var scene_deferred_g0: texture_2d<f32>;
@group(1) @binding(17) var scene_deferred_g1: texture_2d<f32>;
@group(1) @binding(18) var scene_deferred_g2: texture_2d<f32>;
@group(1) @binding(19) var scene_deferred_g3: texture_2d<u32>;
@group(1) @binding(20) var scene_deferred_depth: texture_depth_2d;
@group(1) @binding(21) var<storage, read> scene_lighting_records: array<SceneLightingRecord>;
@group(1) @binding(22) var<uniform> scene_deferred_camera: SceneDeferredCamera;

@vertex
fn vs_scene_deferred_lighting(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    return vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
}

// The homogeneous scene position retains 1 / clip w before perspective division.
fn scene_deferred_homogeneous_position(pixel: vec2<f32>, depth: f32) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(scene_deferred_depth));
    let ndc = vec2<f32>(pixel.x / size.x * 2.0 - 1.0, 1.0 - pixel.y / size.y * 2.0);
    let inverse = scene_deferred_camera.scene_from_clip;
    let scene = fma(inverse[0], vec4<f32>(ndc.x),
        fma(inverse[1], vec4<f32>(ndc.y),
            fma(inverse[2], vec4<f32>(depth - 1.0), inverse[3])));
    return scene;
}

// The scene position of a pixel centre at stored depth, through the same projection the
// producers rasterised with.
fn scene_deferred_position(pixel: vec2<f32>, depth: f32) -> vec3<f32> {
    let scene = scene_deferred_homogeneous_position(pixel, depth);
    return scene.xyz / scene.w;
}

struct SceneDeferredSurface {
    position: vec3<f32>,
    reciprocal_w: f32,
    geometric_normal: vec3<f32>,
    surface: SceneShadingSurface,
    record: SceneLightingRecord,
    // The per-pixel back-scattering amount and the emission intensity.
    amounts: vec2<f32>,
    covered: bool,
}

fn scene_deferred_surface(pixel: vec4<f32>) -> SceneDeferredSurface {
    let texel = vec2<i32>(pixel.xy);
    let g3 = textureLoad(scene_deferred_g3, texel, 0).r;
    let stored = scene_gbuffer_decode(textureLoad(scene_deferred_g0, texel, 0),
        textureLoad(scene_deferred_g1, texel, 0), textureLoad(scene_deferred_g2, texel, 0), g3,
        scene_deferred_camera.scene_from_view);
    // The scattering model stores its per-pixel amount where other surfaces store metal.
    let scattering = (stored.flags & SCENE_GBUFFER_FLAG_SCATTERING) != 0u;
    let metal = select(stored.metal, 0.0, scattering);
    let amount = select(0.0, stored.metal, scattering);
    let scene = scene_deferred_homogeneous_position(pixel.xy,
        textureLoad(scene_deferred_depth, texel, 0));
    return SceneDeferredSurface(
        scene.xyz / scene.w,
        scene.w,
        stored.geometric_normal,
        SceneShadingSurface(vec4<f32>(stored.base_colour, 1.0), stored.shading_normal,
            vec3<f32>(stored.roughness, metal, stored.ao)),
        scene_lighting_records[stored.lighting_record],
        vec2<f32>(amount, stored.emission),
        g3 != 0u,
    );
}

var<private> scene_deferred_geometric_normal: vec3<f32>;
// The lit pixel as a rasterised fragment: pixel centre, no depth, and 1 / clip w. Front's
// receiver-plane solve takes the forward terrain footprint's fragment.
var<private> scene_deferred_fragment: vec4<f32>;
var<private> scene_deferred_covered: bool;

fn scene_deferred_receiver_normal(shading: vec3<f32>, geometric: vec3<f32>, renders: bool) -> vec3<f32> {
    return select(shading, geometric, renders);
}

// Exactly the forward mesh shading after decals (scene_finish_shading), on stored values.
fn scene_deferred_shade(pixel: vec4<f32>) -> vec4<f32> {
    let deferred = scene_deferred_surface(pixel);
    scene_deferred_geometric_normal = deferred.geometric_normal;
    scene_deferred_fragment = vec4<f32>(pixel.xy, 0.0, deferred.reciprocal_w);
    scene_deferred_covered = deferred.covered;
    // SCENE_DEFERRED_PIXEL
    let position = deferred.position;
    let normal = deferred.surface.normal;
    let albedo_sample = deferred.surface.albedo;
    let record = deferred.record;
    let masked_amount = deferred.amounts.x;
    let emission = deferred.amounts.y;
    let lit_visibility = sunlight_visibility(position, normal);
    let far_visibility = scene_deferred_far_visibility(position, -normal);
    let lighting = scene_surface_lighting(position, normal, albedo_sample.rgb,
        deferred.surface.properties, DIELECTRIC_F0);
    var colour = lighting.color;
    if scene_record_scatters(record) {
        colour += scene_record_scattering(position, normal, albedo_sample.rgb, record,
            masked_amount, lit_visibility);
    }
    if scene_record_transmits(record) {
        colour += scene_record_transmission(normal, albedo_sample.rgb, record, far_visibility);
    }
    // Producers store zero emission for surfaces without it, so the term is always added.
    colour += albedo_sample.rgb * emission;
    let exposed = apply_exposure(apply_height_fog(colour, position));
    return select(vec4<f32>(0.0), vec4<f32>(exposed, 1.0), deferred.covered);
}
