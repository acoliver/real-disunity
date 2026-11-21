// The object G-buffer writes: the material shade_material evaluates for the forward path, stored
// instead of lit. Flag bit 0 selects the scattering model (board 112 post 2768): G2.y then holds
// the per-pixel scattering amount and metal is 0. The material's constant scattering shape and
// colour and its translucency reach the lighting pass through the material's lighting record,
// whose index every object stores in G1.a. Impostor cards (SCENE_IMPOSTOR) store
// scene_card_gbuffer_material, which gbuffer_writes derives from the frozen scene_card_shade.
struct SceneGbufferReliefTargets {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
    @location(3) g3: u32,
    @builtin(frag_depth) depth: f32,
}

fn scene_gbuffer_object(input: VertexOutput, evaluated: SceneObjectMaterial) -> SceneGbufferTargets {
    let scattering = material.scattering_mode.y != 0.0;
    let emits = (material.texture_flags & FLAG_SELF_ILLUMINATION) != 0u || scene_packed_atlas(input);
    let surface = SceneGbufferSurface(
        select(evaluated.geometric_normal, normalize(evaluated.surface.normal),
            length(evaluated.surface.normal) > 0.000001),
        evaluated.geometric_normal,
        evaluated.surface.albedo.rgb,
        evaluated.surface.properties.x,
        select(evaluated.surface.properties.y, evaluated.masked_amount, scattering),
        select(0.0, evaluated.emission, emits),
        evaluated.surface.properties.z,
        select(0u, 1u, scattering),
        scene_material_lighting_record_index(),
    );
    let view_from_scene = mat3x3<f32>(screen.view[0].xyz, screen.view[1].xyz, screen.view[2].xyz);
    return scene_gbuffer_encode(surface, view_from_scene);
}

fn scene_gbuffer_relief(targets: SceneGbufferTargets, depth: f32) -> SceneGbufferReliefTargets {
    return SceneGbufferReliefTargets(targets.g0, targets.g1, targets.g2, targets.g3, depth);
}

@fragment
fn fs_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneGbufferTargets {
    let evaluated = scene_object_gbuffer_material(input, front_facing, scene_relief_position(input), false, true);
    return scene_gbuffer_object(input, evaluated);
}

@fragment
fn fs_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneGbufferTargets {
    let evaluated = scene_object_gbuffer_material(input, front_facing, scene_relief_position(input), true, true);
    return scene_gbuffer_object(input, evaluated);
}

@fragment
fn fs_relief_opaque(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneGbufferReliefTargets {
    let depth = scene_relief_depth(input);
    if SCENE_IMPOSTOR {
        let card = scene_card_gbuffer_material(input, front_facing, depth.position, false, depth.visible);
        return scene_gbuffer_relief(scene_gbuffer_object(input, card), depth.value);
    }
    let evaluated = scene_object_gbuffer_material(input, front_facing, depth.position, false, depth.visible);
    return scene_gbuffer_relief(scene_gbuffer_object(input, evaluated), depth.value);
}

@fragment
fn fs_relief_masked(input: VertexOutput, @builtin(front_facing) front_facing: bool) -> SceneGbufferReliefTargets {
    let depth = scene_relief_depth(input);
    if SCENE_IMPOSTOR {
        let card = scene_card_gbuffer_material(input, front_facing, depth.position, true, depth.visible);
        return scene_gbuffer_relief(scene_gbuffer_object(input, card), depth.value);
    }
    let evaluated = scene_object_gbuffer_material(input, front_facing, depth.position, true, depth.visible);
    return scene_gbuffer_relief(scene_gbuffer_object(input, evaluated), depth.value);
}
