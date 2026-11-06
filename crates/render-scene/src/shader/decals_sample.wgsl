    let gradient_scale = exp2(frame.lighting_options.w);
    for (var i = 0u; i < scene_decals.count.x; i++) {
        let decal = scene_decals.volumes[i];
        if (decal.selection.x & receiver) == 0u { continue; }
        let local = (decal.local_from_world * vec4<f32>(position, 1.0)).xyz;
        let edge = 0.5 - max(max(abs(local.x), abs(local.y)), abs(local.z));
        if edge < 0.0 { continue; }
        let angle_fade = scene_decal_angle(dot(geometric, decal.direction_edge.xyz), decal.surface_angle.zw, decal.reserved.yzw);
        var weight = 1.0;
        if angle_fade == 0.0 { continue; }
        if decal.direction_edge.w > 0.0 { weight *= clamp(edge / decal.direction_edge.w, 0.0, 1.0); }
        let uv = (scene_decal_uv(local, decal.selection.z) + 0.5) * decal.uv_transform.xy + decal.uv_transform.zw;
        let dx = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dx, 0.0)).xyz, decal.selection.z) * decal.uv_transform.xy * gradient_scale;
        let dy = scene_decal_uv((decal.local_from_world * vec4<f32>(world_dy, 0.0)).xyz, decal.selection.z) * decal.uv_transform.xy * gradient_scale;
        var sample = textureSampleGrad(scene_decal_albedo, scene_decal_sampler, uv, i32(decal.selection.y), dx, dy);
        sample.a *= angle_fade;
        if (decal.selection.x & 4194304u) != 0u {
            let cutoff = f32((decal.selection.x >> 2u) & 1048575u) / 1000000.0;
            if sample.a < cutoff { continue; }
        }
        let colored = scene_decal_color(sample, decal.color_modifier, (decal.selection.w & 1048576u) != 0u);
        weight *= colored.a * decal.reserved.x;
        if (decal.selection.w & 65536u) == 0u {
            result.albedo = mix(result.albedo, colored.rgb, weight);
        }
        var surface = decal.surface_angle.xy;
        let surface_layer = (decal.selection.w >> 8u) & 255u;
        if surface_layer != 0u {
            surface = textureSampleGrad(scene_decal_surface, scene_decal_sampler, uv, i32(surface_layer - 1u), dx, dy).rb;
        }
        if (decal.selection.w & 262144u) == 0u {
            result.properties = mix(result.properties, surface, weight);
        }
        let normal_layer = decal.selection.w & 255u;
        if normal_layer != 0u && weight > 0.0 && (decal.selection.w & 131072u) == 0u {
            let encoded = textureSampleGrad(scene_decal_normal, scene_decal_sampler, uv, i32(normal_layer - 1u), dx, dy).rg;
            let detail = scene_decal_correct_normal(decal, geometric, encoded);
            result.normal = scene_decal_blend_normal(result.normal, detail, weight);
        }
    }
    return result;
}
