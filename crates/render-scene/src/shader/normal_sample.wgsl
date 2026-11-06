    if SCENE_IMPOSTOR {
        if material.impostor_grid.w != 0.0 {
            if input.atlas_eye.w != 0.0 && (material.impostor_relief.x != 0.0 || scene_packed_atlas(input)) { return scene_relief_normal(input, footprint); }
        }
    }
    let sampled = scene_atlas_sample(normal_texture, input, footprint).rgb * 2.0 - 1.0;
    var xy = sampled.xy;
    var z = sqrt(max(0.0, 1.0 - dot(xy, xy)));
    if (material.geometry_flags.w & 1u) != 0u { z = sampled.z; }
    if (material.geometry_flags.w & 2u) != 0u { xy.y = -xy.y; }
    let scale = bitcast<f32>(material.geometry_flags.z);
    if scale == 0.0 { return geometric; }
    xy *= scale;
    if tangent.w != 0.0 {
        let t = normalize(tangent.xyz - geometric * dot(geometric, tangent.xyz));
        frame_basis = mat3x3<f32>(t, cross(geometric, t) * tangent.w, geometric);
    }
    return normalize(frame_basis * vec3<f32>(xy, z));
}
