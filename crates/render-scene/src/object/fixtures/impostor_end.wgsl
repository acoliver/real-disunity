
fn scene_atlas_inset(dimensions: vec2<f32>) -> vec2<f32> {
    let border = vec2<f32>(material.impostor_grid.z);
    if (material.wind_clock.z & SCENE_ATLAS_MIP_FLAG) != 0u { return border; }
    return max(border, 0.5 * material.impostor_grid.xy / dimensions);
}
