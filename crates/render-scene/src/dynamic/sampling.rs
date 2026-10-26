use worldrunner_render_plan::{TextureFilter, TextureSampler, TextureWrap};
pub(super) fn create_sampler(device: &wgpu::Device, state: TextureSampler) -> wgpu::Sampler {
    let filter = match state.filter {
        TextureFilter::Point => wgpu::FilterMode::Nearest,
        TextureFilter::Bilinear | TextureFilter::Trilinear => wgpu::FilterMode::Linear,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("dynamic texture sampler"),
        address_mode_u: address(state.wrap_u),
        address_mode_v: address(state.wrap_v),
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: match state.filter {
            TextureFilter::Point | TextureFilter::Bilinear => wgpu::MipmapFilterMode::Nearest,
            TextureFilter::Trilinear => wgpu::MipmapFilterMode::Linear,
        },
        ..Default::default()
    })
}
fn address(wrap: TextureWrap) -> wgpu::AddressMode {
    match wrap {
        TextureWrap::Repeat => wgpu::AddressMode::Repeat,
        TextureWrap::Clamp => wgpu::AddressMode::ClampToEdge,
        TextureWrap::Mirror => wgpu::AddressMode::MirrorRepeat,
    }
}
