// The terrain G-buffer write: the material terrain_material evaluates for the forward path,
// stored instead of lit. Terrain has no emission and no material AO map.
@fragment
fn fs_terrain(input: TerrainVertexOutput) -> SceneGbufferTargets {
    let material = terrain_material(input);
    let surface = SceneGbufferSurface(
        material.projected.normal,
        material.geometric,
        material.projected.albedo,
        material.projected.properties.x,
        material.projected.properties.y,
        0.0,
        1.0,
        0u,
    );
    let view_from_scene = mat3x3<f32>(screen.view[0].xyz, screen.view[1].xyz, screen.view[2].xyz);
    return scene_gbuffer_encode(surface, view_from_scene);
}
