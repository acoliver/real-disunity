use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Device,
    ShaderStages, TextureSampleType, TextureViewDimension,
};

use crate::terrain::pages::layered_gpu::textures::ARRAY_LIMIT;

pub(super) const PLANE_BINDING: u32 = 23;

pub(super) fn terrain_layout_of(device: &Device, cached: bool, layered: bool) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("terrain"),
        entries: &terrain_entries(cached, layered),
    })
}

pub(super) fn terrain_layout(device: &Device, cached: bool, layered: bool) -> BindGroupLayout {
    terrain_layout_of(device, cached, layered)
}

fn decal_texture_entry(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: true },
            view_dimension: TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

fn terrain_entries(cached: bool, layered: bool) -> Vec<BindGroupLayoutEntry> {
    let mut entries = vec![BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    entries.push(BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Uint,
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    });
    entries.push(terrain_plane_entry());
    if !layered {
        entries.extend((2..=4).map(decal_texture_entry));
    }
    entries.push(BindGroupLayoutEntry {
        binding: 5,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    for binding in 8..if layered { 11 } else { 8 } {
        entries.push(BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
    }
    if layered {
        entries.extend(draw_layout());
    }
    if cached {
        entries.retain(|entry| entry.binding != 1);
        entries.extend(cached_terrain_entries());
    }
    entries
}

fn terrain_plane_entry() -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding: PLANE_BINDING,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn cached_terrain_entries() -> Vec<BindGroupLayoutEntry> {
    vec![
        BindGroupLayoutEntry {
            binding: 6,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Uint,
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]
}

fn draw_layout() -> Vec<BindGroupLayoutEntry> {
    let mut entries: Vec<_> = (0..ARRAY_LIMIT)
        .map(|index| BindGroupLayoutEntry {
            binding: 11 + index,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    entries.extend((21..23).map(|binding| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }));
    entries
}
