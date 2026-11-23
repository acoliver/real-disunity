//! A view a frame's preparation writes: produced before the frame's first pass, versioned like an
//! attachment's output, and imported at the allocation's own initial version.
use super::*;
use crate::{
    AttachmentDeclaration, AttachmentView, FeatureDeclaration, FeaturePass, FrameError,
    PassDeclaration, PassSlot, ResourceRef,
};

const PREPARED: ResourceRef = ResourceRef {
    name: "prepared",
    version: 0,
    mip: 0,
    layer: 1,
};
const OUTPUT: ResourceRef = ResourceRef::new("output");

fn texture(device: &wgpu::Device, layers: u32, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("prepared write fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | usage,
        view_formats: &[],
    })
}

fn attachment(resource: ResourceRef, load: AttachmentLoad) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource,
        format: wgpu::TextureFormat::Rgba8Unorm,
        size: [4, 4],
        samples: 1,
        load,
        store: wgpu::StoreOp::Store,
        read_only: false,
    }
}

/// One Opaque pass that clears the output, and a feature in it that samples the prepared view.
fn frame() -> (PassDeclaration, FeatureDeclaration) {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = Some(attachment(
        OUTPUT,
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
    ));
    let mut reader = FeaturePass::new(PassSlot::Opaque);
    reader.reads = vec![PREPARED];
    reader.color_format = Some(wgpu::TextureFormat::Rgba8Unorm);
    let feature = FeatureDeclaration {
        passes: vec![reader],
        effects: Vec::new(),
    };
    (pass, feature)
}

/// The views of the frame: the output and `prepared`.
fn views(device: &wgpu::Device, prepared: AttachmentView) -> Vec<AttachmentView> {
    let output = texture(device, 1, wgpu::TextureUsages::RENDER_ATTACHMENT);
    vec![AttachmentView::new(OUTPUT, &output).unwrap(), prepared]
}

/// The version the frame's preparation writes the prepared view at, after a capture imported
/// its texture at `initial`.
fn compile(
    device: &wgpu::Device,
    prepared: AttachmentView,
    initial: Option<u32>,
) -> Result<Vec<u32>, ContractError> {
    let (pass, feature) = frame();
    let mut registry = ResourceRegistry::default();
    registry.import(
        prepared.texture(),
        ResourceLifetime::FrameLocal,
        Semantic::Unspecified,
        initial,
        &[],
    )?;
    let views = views(device, prepared);
    crate::contracts::stock::compile(device, &[pass], &views, &[feature], &mut registry)
        .map(|raster| raster.prepared_versions())
}

#[test]
fn a_prepared_view_keeps_the_allocations_initial_version() {
    let (device, _queue) = device();
    let blocks = texture(&device, 2, wgpu::TextureUsages::COPY_DST);
    let view = AttachmentView::new(PREPARED, &blocks)
        .unwrap()
        .with_prepared_contents();
    assert!(view.prepared_contents());
    assert!(
        !view.allocation_initialized(),
        "a prepared write is no contents an earlier submission left"
    );
    assert_eq!(compile(&device, view.clone(), None), Ok(vec![0]));
    assert!(matches!(
        compile(&device, view, Some(0)),
        Err(ContractError::Identity(_))
    ));
}

#[test]
fn a_prepared_write_over_an_initialized_allocation_takes_the_next_version() {
    let (device, _queue) = device();
    let blocks = texture(&device, 2, wgpu::TextureUsages::COPY_DST);
    let view = AttachmentView::new(PREPARED, &blocks)
        .unwrap()
        .with_initialized_allocation()
        .with_prepared_contents();
    assert_eq!(compile(&device, view, Some(0)), Ok(vec![1]));
}

#[test]
fn a_prepared_write_needs_a_copy_destination() {
    let (device, _queue) = device();
    let blocks = texture(&device, 2, wgpu::TextureUsages::empty());
    let view = AttachmentView::new(PREPARED, &blocks)
        .unwrap()
        .with_prepared_contents();
    assert!(matches!(
        compile(&device, view, None),
        Err(ContractError::Binding(_))
    ));
}

#[test]
fn a_prepared_view_counts_as_produced_and_no_pass_may_clear_it() {
    let (device, _queue) = device();
    let blocks = texture(
        &device,
        2,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let plain = AttachmentView::new(PREPARED, &blocks).unwrap();
    let (pass, feature) = frame();
    let validate = |passes: &[PassDeclaration], view: &AttachmentView| {
        crate::frame::validate_frame_inputs(
            passes,
            std::slice::from_ref(&feature),
            &[],
            &views(&device, view.clone()),
            &[],
        )
    };
    assert_eq!(
        validate(std::slice::from_ref(&pass), &plain),
        Err(FrameError::MissingProducer(PREPARED))
    );
    let prepared = plain.with_prepared_contents();
    assert_eq!(validate(std::slice::from_ref(&pass), &prepared), Ok(()));
    let mut clear = PassDeclaration::new(PassSlot::DepthNormalPrepass);
    clear.color = Some(attachment(
        PREPARED,
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
    ));
    assert_eq!(
        validate(&[clear, pass], &prepared),
        Err(FrameError::AttachmentHazard(PREPARED))
    );
}
