// Compact deferred G-buffer: the shared encoding of one opaque surface into the four
// OpaqueGbuffer targets, and its decoding for the lighting pass.
//
// The module declares no bindings. A producer passes its own camera rotation and
// returns `SceneGbufferTargets` from a fragment entry point drawn in the
// OpaqueGbuffer slot, whose colour attachments are, by location:
//   0 G0 Rgb10a2Unorm   rg: view-space shading normal, Lambert azimuthal equal-area
//                       (spheremap) projection about view +z; b and a reserved, written 0.
//   1 G1 Rgba8UnormSrgb rgb: linear base colour (the target stores its sRGB encoding);
//                       a: the 8-bit lighting-record index, stored as index / 255 (alpha
//                       is not sRGB encoded). Record 0 is the neutral standard model.
//   2 G2 Rgba8Unorm     roughness, metal, sqrt(emission), material AO.
//   3 G3 R16Uint        bits 9..15 and 2..8: the scene-space geometric normal as a 7+7 bit
//                       octahedral code (x high, y low); bits 0..1: the two engine flags.
//                       Flag bit 0 selects the scattering model: G2.g then holds the
//                       per-pixel scattering amount and the surface has no metal. Bit 1 is
//                       reserved.
// Every target clears to zero, and a covered pixel never stores G3 == 0, so G3 == 0 means
// that no producer wrote the pixel.
//
// View space is the camera's right-handed view space (the camera looks down its -z), so a
// camera-facing normal has z > 0 and the projection's singular point, z = -1, faces
// straight away from the camera. A normal at or within float precision of z = -1 encodes
// on the boundary circle and decodes to (0, 0, -1); the angular error grows towards that
// point (see the render-scene README for the measured bounds).
//
// Emission is stored with a square-root transfer, so low emission keeps more precision:
// stored = sqrt(emission), emission = stored * stored, both in 0..1.

struct SceneGbufferSurface {
    // Scene space, after decals, unit length.
    shading_normal: vec3<f32>,
    // Scene space, unit length.
    geometric_normal: vec3<f32>,
    // Linear; G1 is the sRGB view.
    base_colour: vec3<f32>,
    roughness: f32,
    // Metal, or the per-pixel scattering amount when flag bit 0 selects the scattering model.
    metal: f32,
    // 0..1, stored as its square root.
    emission: f32,
    // Material AO; screen AO stays a lighting input.
    ao: f32,
    // The two engine bits stored in G3 bits 0..1. Higher bits are not stored.
    flags: u32,
    // Index into the frame's lighting-record table, 0..255; 0 is the neutral standard model.
    // Producers copy their material's index (scene_material_lighting_record_index()).
    lighting_record: u32,
}

struct SceneGbufferTargets {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
    @location(3) g3: u32,
}

const SCENE_GBUFFER_FLAG_MASK: u32 = 3u;
// G3 flag bit 0: the scattering model.
const SCENE_GBUFFER_FLAG_SCATTERING: u32 = 1u;
const SCENE_GBUFFER_RECORD_MAX: f32 = 255.0;
const SCENE_GBUFFER_OCT_MAX: f32 = 127.0;
// The octahedral corner codes (0, 0) and (127, 127) both decode to (0, 0, -1).
const SCENE_GBUFFER_OCT_CORNER: u32 = 127u;

// Spheremap projection of a unit view-space normal into 0..1. For a unit vector this
// equals n.xy / sqrt(8 * n.z + 8) + 0.5; the radius form stays defined at n.z = -1.
fn scene_gbuffer_spheremap_encode(normal: vec3<f32>) -> vec2<f32> {
    let planar = length(normal.xy);
    let direction = select(vec2<f32>(1.0, 0.0), normal.xy / planar, planar > 0.0);
    let radius = sqrt(max(1.0 - normal.z, 0.0) * 0.125);
    return direction * radius + vec2<f32>(0.5, 0.5);
}

// Inverse of the projection. Codes outside the unit-sphere disc decode to (0, 0, -1).
fn scene_gbuffer_spheremap_decode(encoded: vec2<f32>) -> vec3<f32> {
    let planar = encoded * 4.0 - vec2<f32>(2.0, 2.0);
    let f = min(dot(planar, planar), 4.0);
    let scale = sqrt(1.0 - f * 0.25);
    return vec3<f32>(planar * scale, 1.0 - f * 0.5);
}

fn scene_gbuffer_sign(value: vec2<f32>) -> vec2<f32> {
    return select(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), value >= vec2<f32>(0.0, 0.0));
}

// Octahedral projection of a nonzero vector into -1..1.
fn scene_gbuffer_oct_encode(normal: vec3<f32>) -> vec2<f32> {
    let projected = normal.xy / (abs(normal.x) + abs(normal.y) + abs(normal.z));
    let folded = (vec2<f32>(1.0, 1.0) - abs(projected.yx)) * scene_gbuffer_sign(projected);
    return select(projected, folded, normal.z < 0.0);
}

fn scene_gbuffer_oct_decode(encoded: vec2<f32>) -> vec3<f32> {
    let z = 1.0 - abs(encoded.x) - abs(encoded.y);
    let fold = clamp(-z, 0.0, 1.0);
    let xy = encoded - fold * scene_gbuffer_sign(encoded);
    return normalize(vec3<f32>(xy, z));
}

fn scene_gbuffer_g3_encode(normal: vec3<f32>, flags: u32) -> u32 {
    let unit = clamp(scene_gbuffer_oct_encode(normal) * 0.5 + vec2<f32>(0.5, 0.5),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
    var code = vec2<u32>(floor(unit * SCENE_GBUFFER_OCT_MAX + vec2<f32>(0.5, 0.5)));
    let bits = flags & SCENE_GBUFFER_FLAG_MASK;
    if code.x == 0u && code.y == 0u && bits == 0u {
        code = vec2<u32>(SCENE_GBUFFER_OCT_CORNER, SCENE_GBUFFER_OCT_CORNER);
    }
    return (code.x << 9u) | (code.y << 2u) | bits;
}

fn scene_gbuffer_g3_normal(g3: u32) -> vec3<f32> {
    let code = vec2<f32>(f32((g3 >> 9u) & 127u), f32((g3 >> 2u) & 127u));
    return scene_gbuffer_oct_decode(code / SCENE_GBUFFER_OCT_MAX * 2.0 - vec2<f32>(1.0, 1.0));
}

fn scene_gbuffer_encode(surface: SceneGbufferSurface, view_from_scene: mat3x3<f32>) -> SceneGbufferTargets {
    let view_normal = view_from_scene * surface.shading_normal;
    return SceneGbufferTargets(
        vec4<f32>(scene_gbuffer_spheremap_encode(view_normal), 0.0, 0.0),
        vec4<f32>(surface.base_colour, f32(surface.lighting_record) / SCENE_GBUFFER_RECORD_MAX),
        vec4<f32>(surface.roughness, surface.metal, sqrt(max(surface.emission, 0.0)), surface.ao),
        scene_gbuffer_g3_encode(surface.geometric_normal, surface.flags),
    );
}

fn scene_gbuffer_decode(
    g0: vec4<f32>,
    g1: vec4<f32>,
    g2: vec4<f32>,
    g3: u32,
    scene_from_view: mat3x3<f32>,
) -> SceneGbufferSurface {
    return SceneGbufferSurface(
        normalize(scene_from_view * scene_gbuffer_spheremap_decode(g0.xy)),
        scene_gbuffer_g3_normal(g3),
        g1.rgb,
        g2.r,
        g2.g,
        g2.b * g2.b,
        g2.a,
        g3 & SCENE_GBUFFER_FLAG_MASK,
        u32(g1.a * SCENE_GBUFFER_RECORD_MAX + 0.5),
    );
}
