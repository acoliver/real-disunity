// Sampling function from main daf4638, for same-adapter byte comparison.
fn cached_surface(world: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> BlendedSurface {
    let global = clamp((world - page_table.bounds.xy) / page_table.bounds.zw, vec2<f32>(0.0), vec2<f32>(1.0));
    let interior = vec2<f32>(page_table.shape.xy);
    let footprint = max(length(dx / page_table.bounds.zw * interior), length(dy / page_table.bounds.zw * interior));
    let result = lookup_page(global, footprint);
    let chosen = result.chosen;
    let lod = result.lod;
    let local = global * exp2(f32(chosen.z)) - vec2<f32>(chosen.xy);
    let border = f32(page_table.shape.z);
    let uv = (local * interior + vec2<f32>(border)) / (interior + vec2<f32>(2.0 * border));
    let color = page_filtered(uv, i32(chosen.w * (2u + page_table.counts.z)), lod);
    let normal = page_filtered(uv, i32(chosen.w * (2u + page_table.counts.z) + 1u), lod);
    return BlendedSurface(color.rgb, normalize(normal.xyz * 2.0 - 1.0), normal.w, select(0.0, color.a, page_table.counts.z != 0u));
}
