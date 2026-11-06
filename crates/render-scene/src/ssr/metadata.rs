pub(super) fn material_texture(
    device: &wgpu::Device,
    size: [u32; 2],
    screen: &crate::scene_metadata::SceneMetadata,
) -> Result<wgpu::Texture, worldrunner_render_post::FrameError> {
    let material = crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Rgba16Float);
    crate::frame::metadata::validate_inputs(
        &screen.depth,
        &screen.normals,
        Some(&material),
        None,
        crate::frame::metadata::MetadataSampling::PixelCenter,
    )
    .map_err(|_| worldrunner_render_post::FrameError::SsrSettings("source metadata contract"))?;
    Ok(material)
}

/// Builds the pipelines the prepass draws with when it also writes the reflection material.
/// They share the explicit group layout of the normal-only prepass pipelines.
pub(super) fn install_merged_pipelines(
    device: &wgpu::Device,
    screen: &mut crate::scene_metadata::SceneMetadata,
) {
    screen.geometry.merged = Some(
        crate::scene_metadata::pipelines(
            device,
            &screen.uniform,
            crate::scene_metadata::MetadataOutput::Merged,
        )
        .0,
    );
}
