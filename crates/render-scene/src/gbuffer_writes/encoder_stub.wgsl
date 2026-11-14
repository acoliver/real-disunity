// Stand-in for the shared G-buffer encoder of restructure step 1 (board 112 post 2719), with its
// names and signature. Its encoding is a placeholder; only the interface is relied on.
struct SceneGbufferSurface {
    shading_normal: vec3<f32>,
    geometric_normal: vec3<f32>,
    base_colour: vec3<f32>,
    roughness: f32,
    metal: f32,
    emission: f32,
    ao: f32,
    flags: u32,
}

struct SceneGbufferTargets {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
    @location(3) g3: u32,
}

fn scene_gbuffer_encode(surface: SceneGbufferSurface, view_from_scene: mat3x3<f32>) -> SceneGbufferTargets {
    let normal = view_from_scene * surface.shading_normal;
    return SceneGbufferTargets(
        vec4<f32>(normal * 0.5 + 0.5, 0.0),
        vec4<f32>(surface.base_colour, 0.0),
        vec4<f32>(surface.roughness, surface.metal, surface.emission, surface.ao),
        surface.flags,
    );
}
