fn scene_prepare_surface_channels(input: VertexOutput) -> AtlasFootprint {
    var footprint: AtlasFootprint;
    if (material.texture_flags & FLAG_ROUGH_METAL) == 0u { return footprint; }
    return scene_atlas_footprint(rough_metal_texture, input);
}
fn scene_surface_channels_prepared(input: VertexOutput, footprint: AtlasFootprint) -> vec2<f32> {
    if (material.texture_flags & FLAG_ROUGH_METAL) == 0u { return vec2<f32>(material.roughness, material.metalness); }
