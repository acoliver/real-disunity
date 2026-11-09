use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer,
    Device, Sampler, Texture, TextureView, TextureViewDescriptor, TextureViewDimension,
};

use super::layouts::PLANE_BINDING;
use crate::terrain::pages::layered_gpu::textures::ResidentSource;

/// The resources the fixed bind groups are built from, held by the scene's terrain.
pub(crate) struct Resources<'a> {
    pub uniform: &'a Buffer,
    pub grid: &'a Texture,
    pub palettes: &'a [Texture],
    pub layered: Option<&'a ResidentSource>,
    pub planes: &'a Buffer,
    pub sampler: &'a Sampler,
}

/// Binds the terrain uniform, the type grid, the three palette arrays and the sampler into the
/// one group the terrain shader reads.
pub(crate) fn terrain_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    resources: &Resources<'_>,
    pages: Option<(&TextureView, &Buffer)>,
) -> BindGroup {
    let grid_view = resources
        .grid
        .create_view(&TextureViewDescriptor::default());
    let palette_views = palette_views(resources);

    let mut entries = vec![
        BindGroupEntry {
            binding: 0,
            resource: resources.uniform.as_entire_binding(),
        },
        BindGroupEntry {
            binding: 1,
            resource: BindingResource::TextureView(&grid_view),
        },
    ];
    entries.push(BindGroupEntry {
        binding: PLANE_BINDING,
        resource: resources.planes.as_entire_binding(),
    });
    if pages.is_some() {
        entries.retain(|entry| entry.binding != 1);
    }
    for (slot, view) in palette_views.iter().enumerate() {
        entries.push(BindGroupEntry {
            binding: u32::try_from(slot + 2)
                .expect("three palette slots number far below u32::MAX"),
            resource: BindingResource::TextureView(view),
        });
    }
    entries.push(BindGroupEntry {
        binding: 5,
        resource: BindingResource::Sampler(resources.sampler),
    });
    if let Some((view, table)) = pages {
        entries.push(BindGroupEntry {
            binding: 6,
            resource: BindingResource::TextureView(view),
        });
        entries.push(BindGroupEntry {
            binding: 7,
            resource: table.as_entire_binding(),
        });
    }
    if let Some(source) = resources.layered {
        entries.retain(|entry| !matches!(entry.binding, 2..=4));
        entries.extend(layered_entries(source));
    }
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("terrain"),
        layout,
        entries: &entries,
    })
}

/// A 2D array view of each palette texture.
fn palette_views(resources: &Resources<'_>) -> Vec<TextureView> {
    resources
        .palettes
        .iter()
        .map(|texture| {
            texture.create_view(&TextureViewDescriptor {
                dimension: Some(TextureViewDimension::D2Array),
                ..TextureViewDescriptor::default()
            })
        })
        .collect()
}

fn layered_entries(source: &ResidentSource) -> Vec<BindGroupEntry<'_>> {
    let mut entries: Vec<_> = source
        .buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| BindGroupEntry {
            binding: 8 + u32::try_from(index).expect("three inputs"),
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.extend(
        source
            .views
            .iter()
            .enumerate()
            .map(|(index, view)| BindGroupEntry {
                binding: 11 + u32::try_from(index).expect("texture slot"),
                resource: BindingResource::TextureView(view),
            }),
    );
    entries.extend(
        source
            .samplers
            .iter()
            .enumerate()
            .map(|(index, sampler)| BindGroupEntry {
                binding: 21 + u32::try_from(index).expect("sampler slot"),
                resource: BindingResource::Sampler(sampler),
            }),
    );
    entries
}
