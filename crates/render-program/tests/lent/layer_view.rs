//! One layer of a 2D array lent through a 2D view of that layer: a storage-image binding of
//! 2D takes it, as a sampled binding does, while the whole array is still refused.

use wgpu::ShaderStages;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::{ProgramError, StorageImageError};
use worldrunner_render_program::runtime::{Import, ImportTable};

use crate::support::Gpu;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

/// A four-layer `R32Float` array a kernel may write.
fn array(gpu: &Gpu) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("four layers"),
        size: wgpu::Extent3d {
            width: 8,
            height: 4,
            depth_or_array_layers: 4,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn semantic() -> EngineSemantic {
    EngineSemantic::named("layer.written")
}

/// A set of one 2D storage image a kernel writes.
fn storage_set() -> ResourceLayout {
    ResourceLayout::new(vec![ResourceBinding {
        at: BindingAddress { set: 0, binding: 0 },
        source: BindingSource::Engine(semantic()),
        visibility: ShaderStages::COMPUTE,
        kind: ResourceKind::StorageImage {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: FORMAT,
            dimension: wgpu::TextureViewDimension::D2,
        },
    }])
}

#[test]
fn a_two_dimensional_view_of_one_layer_of_an_array_suits_a_two_dimensional_storage_image() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let set = runtime.external_set(storage_set()).unwrap();
    let texture = array(&gpu);
    let layer = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_array_layer: 2,
        array_layer_count: Some(1),
        ..wgpu::TextureViewDescriptor::default()
    });
    let lent = ImportTable::new().with(Import::texture_view(
        semantic(),
        texture.clone(),
        layer,
        wgpu::TextureViewDimension::D2,
    ));
    runtime.prepare_external_set(&set, &lent).unwrap();
    let whole = ImportTable::new().with(Import::texture(
        semantic(),
        texture.clone(),
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
    ));
    assert!(
        matches!(
            runtime.prepare_external_set(&set, &whole),
            Err(ProgramError::StorageImage {
                set: 0,
                binding: 0,
                error: StorageImageError::Dimension {
                    declared: wgpu::TextureViewDimension::D2,
                    found: wgpu::TextureDimension::D2,
                    layers: 4,
                },
            })
        ),
        "the whole array is not a 2D storage image"
    );
}
