    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if scene_packed_atlas(input) {
                let sampled = scene_atlas_first(rough_metal_texture, footprint);
                return vec2<f32>(sampled[material.packed_atlas_channels[0].x + 0], sampled[material.packed_atlas_channels[1].x + 1]);
            }
        }
    }
    let sampled = scene_atlas_sample(rough_metal_texture, input, footprint);
    return vec2<f32>(material.roughness * sampled[material.scalar_channels[0].x + 1], material.metalness * sampled[material.scalar_channels[1].x + 2]);
}
