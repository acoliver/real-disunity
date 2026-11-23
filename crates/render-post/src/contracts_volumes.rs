use super::*;

/// A 3D texture of `depth` slices that a draw samples.
fn volume(device: &wgpu::Device, depth: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("volume fixture"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: depth,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn imported(registry: &mut ResourceRegistry, texture: &wgpu::Texture) -> AllocationKey {
    registry
        .import(
            texture,
            ResourceLifetime::ImportedReadOnly,
            Semantic::LinearColor,
            Some(0),
            &[],
        )
        .unwrap()
}

fn range() -> TextureRange {
    TextureRange::single(0, 0, wgpu::TextureAspect::All)
}

fn viewed(
    registry: &ResourceRegistry,
    key: AllocationKey,
    dimension: wgpu::TextureViewDimension,
) -> Result<CheckedView, ContractError> {
    registry.view_dimension(key, range(), 0, dimension)
}

#[test]
fn a_one_slice_volume_is_sampled_through_a_3d_view() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = imported(&mut registry, &volume(&device, 1));
    let view = viewed(&registry, key, wgpu::TextureViewDimension::D3).unwrap();
    assert_eq!(view.dimension(), wgpu::TextureViewDimension::D3);
    registry.check_view(&view).unwrap();
}

#[test]
fn a_volume_is_not_viewed_as_a_2d_texture_and_a_2d_texture_not_as_a_volume() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let flat = imported(&mut registry, &volume(&device, 1));
    for dimension in [
        wgpu::TextureViewDimension::D2,
        wgpu::TextureViewDimension::D2Array,
    ] {
        assert_eq!(
            viewed(&registry, flat, dimension).unwrap_err(),
            ContractError::Range(flat),
            "{dimension:?}"
        );
    }
    let plain = imported(&mut registry, &texture(&device, 1));
    assert_eq!(
        viewed(&registry, plain, wgpu::TextureViewDimension::D3).unwrap_err(),
        ContractError::Range(plain)
    );
}

#[test]
fn a_deeper_volume_is_one_cell_with_a_whole_3d_view() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = imported(&mut registry, &volume(&device, 5));
    let view = viewed(&registry, key, wgpu::TextureViewDimension::D3).unwrap();
    assert_eq!(view.dimension(), wgpu::TextureViewDimension::D3);
    registry.check_view(&view).unwrap();
    for dimension in [
        wgpu::TextureViewDimension::D2,
        wgpu::TextureViewDimension::D2Array,
    ] {
        assert_eq!(
            viewed(&registry, key, dimension).unwrap_err(),
            ContractError::Range(key),
            "{dimension:?}"
        );
    }
}
