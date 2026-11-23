//! How the stock compile imports a texture whose allocation holds contents an earlier submission
//! left: a capture that imported the texture first has to import it the same way.
use super::*;
use crate::{AttachmentDeclaration, AttachmentView, PassDeclaration, PassSlot, ResourceRef};

const LAYER: ResourceRef = ResourceRef {
    name: "kept",
    version: 0,
    mip: 0,
    layer: 1,
};

fn array(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("initialized allocation fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// A frame that clears the second layer of the array and nothing else.
fn compile(
    device: &wgpu::Device,
    texture: &wgpu::Texture,
    view: AttachmentView,
    registry: &mut ResourceRegistry,
) -> Result<(), ContractError> {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(AttachmentDeclaration {
        resource: LAYER,
        format: texture.format(),
        size: [4, 4],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    });
    crate::contracts::stock::compile(device, &[pass], &[view], &[], registry).map(drop)
}

fn captured(registry: &mut ResourceRegistry, texture: &wgpu::Texture, initial: Option<u32>) {
    registry
        .import(
            texture,
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            initial,
            &[],
        )
        .unwrap();
}

#[test]
fn a_frame_attaching_only_an_unwritten_layer_imports_an_initialized_allocation_at_version_zero() {
    let (device, _queue) = device();
    let texture = array(&device);
    let view = AttachmentView::new(LAYER, &texture)
        .unwrap()
        .with_initialized_allocation();
    assert!(view.allocation_initialized());
    assert!(!view.retained(), "the layer itself holds no contents");
    let mut registry = ResourceRegistry::default();
    captured(&mut registry, &texture, Some(0));
    compile(&device, &texture, view.clone(), &mut registry).unwrap();
    let mut registry = ResourceRegistry::default();
    captured(&mut registry, &texture, None);
    assert!(matches!(
        compile(&device, &texture, view, &mut registry),
        Err(ContractError::Identity(_))
    ));
}

#[test]
fn a_frame_over_a_fresh_allocation_imports_it_without_an_initial_version() {
    let (device, _queue) = device();
    let texture = array(&device);
    let view = AttachmentView::new(LAYER, &texture).unwrap();
    assert!(!view.allocation_initialized());
    let mut registry = ResourceRegistry::default();
    captured(&mut registry, &texture, None);
    compile(&device, &texture, view.clone(), &mut registry).unwrap();
    let mut registry = ResourceRegistry::default();
    captured(&mut registry, &texture, Some(0));
    assert!(matches!(
        compile(&device, &texture, view, &mut registry),
        Err(ContractError::Identity(_))
    ));
}

#[test]
fn a_retained_view_declares_an_initialized_allocation() {
    let (device, _queue) = device();
    let texture = array(&device);
    let view = AttachmentView::new(LAYER, &texture)
        .unwrap()
        .with_retained_contents();
    assert!(view.allocation_initialized());
    let mut registry = ResourceRegistry::default();
    captured(&mut registry, &texture, Some(0));
    compile(&device, &texture, view, &mut registry).unwrap();
}
