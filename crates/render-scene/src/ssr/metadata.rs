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
