use crate::frame::FrameUniform;

pub(super) fn group(
    device: &wgpu::Device,
    depth: &wgpu::Texture,
    frame: &FrameUniform,
    size: [u32; 2],
) -> wgpu::BindGroup {
    let matrix = frame.view * frame.view_projection.inverse();
    let extent = glam::UVec2::from_array(size).as_vec2();
    let bytes: Vec<_> = matrix
        .to_cols_array()
        .into_iter()
        .chain([extent.x, extent.y, 0., 0.])
        .flat_map(f32::to_le_bytes)
        .collect();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("annotation depth reconstruction"),
        size: 80,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .expect("mapped annotation uniform")
        .copy_from_slice(&bytes);
    buffer.unmap();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("annotation scene depth"),
        layout: &crate::renderer::overlay::depth_layout(device),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    &depth.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
        ],
    })
}
