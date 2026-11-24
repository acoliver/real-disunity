//! The stock compile hands out the frame's own attachment views: a view the frame already holds
//! for a resource is the view its passes attach and sample, not a second one created per frame.
use super::*;
use crate::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeatureDeclaration, FeaturePass,
    PassDeclaration, PassSlot, ResourceRef,
};

const COLOR: ResourceRef = ResourceRef::new("fixture color");
const SHADOW: ResourceRef = ResourceRef {
    name: "fixture shadow",
    version: 0,
    mip: 0,
    layer: 1,
};

fn texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layers: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("attachment view fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | usage,
        view_formats: &[],
    })
}

fn attachment(resource: ResourceRef, format: wgpu::TextureFormat) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource,
        format,
        size: [4, 4],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
        read_only: false,
    }
}

/// An Opaque pass that clears `COLOR`, and a feature in it that samples `SHADOW` layer 1.
fn frame(format: wgpu::TextureFormat) -> (PassDeclaration, FeatureDeclaration) {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(COLOR, format));
    let mut reader = FeaturePass::new(PassSlot::Opaque);
    reader.reads = vec![SHADOW];
    reader.color_format = Some(format);
    let feature = FeatureDeclaration {
        passes: vec![reader],
        effects: Vec::new(),
    };
    (pass, feature)
}

fn compile(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (Vec<AttachmentView>, crate::contracts::StockRaster) {
    let color = texture(device, format, 1, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let layers = texture(device, format, 2, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let views = vec![
        AttachmentView::new(COLOR, &color).unwrap(),
        AttachmentView::new(SHADOW, &layers)
            .unwrap()
            .with_retained_contents(),
    ];
    let (pass, feature) = frame(format);
    let raster = crate::contracts::stock::compile(
        device,
        &[pass],
        &views,
        &[feature],
        &mut ResourceRegistry::default(),
    )
    .unwrap();
    (views, raster)
}

#[test]
fn an_attachment_the_frame_holds_is_the_view_its_pass_attaches() {
    let (device, _queue) = device();
    let (views, raster) = compile(&device, wgpu::TextureFormat::Rgba8Unorm);
    let attached = raster.color_views();
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0].view(), views[0].view());
}

#[test]
fn a_resource_the_frame_holds_is_the_view_a_pass_samples() {
    let (device, _queue) = device();
    let (views, raster) = compile(&device, wgpu::TextureFormat::Rgba8Unorm);
    let sampled = raster.sampled_views();
    assert_eq!(sampled.len(), 1);
    assert_eq!(sampled[0].view(), views[1].view());
}

#[test]
fn a_reused_view_keeps_the_usages_a_created_one_has() {
    let (device, _queue) = device();
    let (views, raster) = compile(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let attached = raster.color_views();
    assert_eq!(attached[0].usage(), views[0].texture().usage());
    assert_eq!(attached[0].format(), wgpu::TextureFormat::Rgba8UnormSrgb);
}

#[test]
fn a_depth_stencil_view_hands_every_pass_the_same_depth_view_to_sample() {
    let (device, _queue) = device();
    let depth = texture(
        &device,
        wgpu::TextureFormat::Depth24PlusStencil8,
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let view = AttachmentView::new(COLOR, &depth).unwrap();
    let sampled = view.sampled_view();
    assert_eq!(view.sampled_view(), sampled);
    assert_eq!(
        view.clone().with_retained_contents().sampled_view(),
        sampled,
        "a copy of the view the frame holds shares it"
    );
    assert_ne!(sampled, *view.view(), "it selects the depth aspect only");
}
