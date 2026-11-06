    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if scene_packed_atlas(input) { return scene_atlas_first(rough_metal_texture, footprint).rg; }
        }
    }
    let sampled = scene_atlas_sample(rough_metal_texture, input, footprint);
    return vec2<f32>(material.roughness * sampled.g, material.metalness * sampled.b);
}
