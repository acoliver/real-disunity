//! Geometry declarations for the same linear executor as post stages.
use std::collections::BTreeMap;

use crate::FrameTargets;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PassSlot {
    ShadowCasters(u32),
    DepthNormalPrepass,
    Opaque,
    AfterOpaqueSnapshot,
    TransparentWater,
    ScreenSpaceAfterTransparent,
}

/// A specific texture version and subresource, never an implicit latest value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceRef {
    pub name: &'static str,
    pub version: u32,
    pub mip: u32,
    pub layer: u32,
}
impl ResourceRef {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            version: 0,
            mip: 0,
            layer: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttachmentLoad {
    Load,
    ClearColor(wgpu::Color),
    ClearDepth(f32),
}

#[derive(Clone, Debug)]
pub struct AttachmentDeclaration {
    pub resource: ResourceRef,
    pub format: wgpu::TextureFormat,
    pub size: [u32; 2],
    pub samples: u32,
    pub load: AttachmentLoad,
    pub store: wgpu::StoreOp,
}

#[derive(Clone, Debug)]
pub struct PassDeclaration {
    pub slot: PassSlot,
    pub color: Option<AttachmentDeclaration>,
    pub depth: Option<AttachmentDeclaration>,
}
impl PassDeclaration {
    pub const fn new(slot: PassSlot) -> Self {
        Self {
            slot,
            color: None,
            depth: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FeaturePass {
    pub slot: PassSlot,
    pub reads: Vec<ResourceRef>,
    pub color_format: Option<wgpu::TextureFormat>,
    pub depth_format: Option<wgpu::TextureFormat>,
    pub samples: u32,
}
impl FeaturePass {
    pub const fn new(slot: PassSlot) -> Self {
        Self {
            slot,
            reads: Vec::new(),
            color_format: None,
            depth_format: None,
            samples: 1,
        }
    }
}
#[derive(Clone, Debug)]
pub struct FeatureDeclaration {
    pub passes: Vec<FeaturePass>,
    pub effects: Vec<&'static str>,
}
impl FeatureDeclaration {
    pub fn new(slot: PassSlot) -> Self {
        Self {
            passes: vec![FeaturePass::new(slot)],
            effects: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("frame slots must be unique and in execution order")]
    SlotOrder,
    #[error("feature requests an undeclared slot {0:?}")]
    MissingSlot(PassSlot),
    #[error("resource has no preceding producer: {0:?}")]
    MissingProducer(ResourceRef),
    #[error("resource is loaded or sampled after discard: {0:?}")]
    LoadAfterDiscard(ResourceRef),
    #[error("resource is sampled while attached or written ambiguously: {0:?}")]
    AttachmentHazard(ResourceRef),
    #[error("attachment or pipeline contract is incompatible in slot {0:?}")]
    AttachmentContract(PassSlot),
    #[error("frame does not support requested effect {0}")]
    UnsupportedEffect(&'static str),
    #[error("attachment view does not match its declared subresource: {0:?}")]
    AttachmentView(ResourceRef),
}

/// Validates the authored sequence without allocating, preparing or encoding anything.
pub fn validate_frame(
    passes: &[PassDeclaration],
    features: &[FeatureDeclaration],
    requested: &[&'static str],
) -> Result<(), FrameError> {
    if passes.windows(2).any(|pair| pair[0].slot >= pair[1].slot) {
        return Err(FrameError::SlotOrder);
    }
    for effect in requested {
        if !features
            .iter()
            .any(|feature| feature.effects.contains(effect))
        {
            return Err(FrameError::UnsupportedEffect(effect));
        }
    }
    for feature in features {
        for feature_pass in &feature.passes {
            if !passes.iter().any(|pass| pass.slot == feature_pass.slot) {
                return Err(FrameError::MissingSlot(feature_pass.slot));
            }
        }
    }
    let mut produced = BTreeMap::new();
    for pass in passes {
        let attachments: Vec<_> = pass.color.iter().chain(&pass.depth).collect();
        validate_attachments(pass, &produced)?;
        for feature in features
            .iter()
            .flat_map(|feature| &feature.passes)
            .filter(|feature| feature.slot == pass.slot)
        {
            validate_feature(pass, feature, &attachments, &produced)?;
        }
        for attachment in attachments {
            produced.insert(attachment.resource, attachment.clone());
        }
    }
    Ok(())
}
fn validate_attachments(
    pass: &PassDeclaration,
    produced: &BTreeMap<ResourceRef, AttachmentDeclaration>,
) -> Result<(), FrameError> {
    let attachments: Vec<_> = pass.color.iter().chain(&pass.depth).collect();
    if attachments.is_empty() {
        return Err(FrameError::AttachmentContract(pass.slot));
    }
    if let (Some(color), Some(depth)) = (&pass.color, &pass.depth) {
        if color.resource == depth.resource {
            return Err(FrameError::AttachmentHazard(color.resource));
        }
        if color.size != depth.size || color.samples != depth.samples {
            return Err(FrameError::AttachmentContract(pass.slot));
        }
    }
    for attachment in &attachments {
        let depth = pass
            .depth
            .as_ref()
            .is_some_and(|item| std::ptr::eq(item, *attachment));
        if attachment.size.contains(&0)
            || attachment.samples != 1
            || attachment.format.is_depth_stencil_format() != depth
            || (depth && !attachment.format.has_depth_aspect())
            || !attachment
                .format
                .guaranteed_format_features(wgpu::Features::empty())
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || matches!(attachment.load, AttachmentLoad::ClearDepth(value) if !value.is_finite() || !(0.0..=1.0).contains(&value))
            || matches!(
                (depth, attachment.load),
                (true, AttachmentLoad::ClearColor(_)) | (false, AttachmentLoad::ClearDepth(_))
            )
        {
            return Err(FrameError::AttachmentContract(pass.slot));
        }
        if attachment.load == AttachmentLoad::Load {
            let previous = require_producer(produced, attachment.resource)?;
            if previous.format != attachment.format
                || previous.size != attachment.size
                || previous.samples != attachment.samples
            {
                return Err(FrameError::AttachmentContract(pass.slot));
            }
        } else if produced.contains_key(&attachment.resource) {
            return Err(FrameError::AttachmentHazard(attachment.resource));
        }
    }
    Ok(())
}
fn validate_feature(
    pass: &PassDeclaration,
    feature: &FeaturePass,
    attachments: &[&AttachmentDeclaration],
    produced: &BTreeMap<ResourceRef, AttachmentDeclaration>,
) -> Result<(), FrameError> {
    if feature.samples != 1
        || feature
            .color_format
            .is_some_and(|format| pass.color.as_ref().is_none_or(|a| a.format != format))
        || feature
            .depth_format
            .is_some_and(|format| pass.depth.as_ref().is_none_or(|a| a.format != format))
    {
        return Err(FrameError::AttachmentContract(pass.slot));
    }
    for resource in &feature.reads {
        if attachments.iter().any(|a| a.resource == *resource) {
            return Err(FrameError::AttachmentHazard(*resource));
        }
        require_producer(produced, *resource)?;
    }
    Ok(())
}

fn require_producer(
    produced: &BTreeMap<ResourceRef, AttachmentDeclaration>,
    resource: ResourceRef,
) -> Result<&AttachmentDeclaration, FrameError> {
    match produced.get(&resource) {
        None => Err(FrameError::MissingProducer(resource)),
        Some(previous) if previous.store == wgpu::StoreOp::Discard => {
            Err(FrameError::LoadAfterDiscard(resource))
        }
        Some(previous) => Ok(previous),
    }
}

pub struct BuildContext<'a> {
    pub device: &'a wgpu::Device,
    pub targets: &'a FrameTargets,
}
pub struct PrepareContext<'a> {
    device: &'a wgpu::Device,
    encoder: &'a mut wgpu::CommandEncoder,
    copies: Vec<(wgpu::Buffer, u64, wgpu::Buffer, u64, u64)>,
}
impl<'a> PrepareContext<'a> {
    pub fn new(device: &'a wgpu::Device, encoder: &'a mut wgpu::CommandEncoder) -> Self {
        Self {
            device,
            encoder,
            copies: Vec::new(),
        }
    }
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }
    pub fn copy_buffer(&mut self, source: &wgpu::Buffer, destination: &wgpu::Buffer, bytes: u64) {
        self.copies
            .push((source.clone(), 0, destination.clone(), 0, bytes));
    }
    pub fn upload_buffer(&mut self, destination: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        use wgpu::util::DeviceExt;
        let staging = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("frame upload"),
                contents: bytes,
                usage: wgpu::BufferUsages::COPY_SRC,
            });
        self.copies
            .push((staging, 0, destination.clone(), offset, bytes.len() as u64));
    }
    /// Publishes staged copies only after all fallible preparations succeed.
    pub fn finish(self) {
        for (source, source_offset, destination, offset, bytes) in self.copies {
            self.encoder
                .copy_buffer_to_buffer(&source, source_offset, &destination, offset, bytes);
        }
    }
}
/// Later geometry slices will add view identifiers/bindings and validated
/// inter-slot compute, copy and fullscreen operations. Those operations are not
/// part of this geometry-only interface; stock effect validation remains separate.
pub struct DrawContext {
    pub slot: PassSlot,
}

/// Features receive neither a queue nor authority to open or replace attachments.
pub trait FrameFeature {
    fn declare(&self) -> FeatureDeclaration;
    fn build(&self, context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError>;
}
/// Preparation may stage uploads, but must not publish resident state.
/// Publish state in `commit`, which runs only after every preparation succeeds.
pub trait PreparedFeature {
    fn prepare(&mut self, context: &mut PrepareContext<'_>) -> Result<(), FrameError>;
    fn commit(&mut self, _context: &mut PrepareContext<'_>) {}
    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureId(pub(crate) usize);

pub(crate) struct RegisteredFeature {
    pub declaration: FeatureDeclaration,
    pub prepared: Option<Box<dyn PreparedFeature>>,
    pub source: Box<dyn FrameFeature>,
}

/// Borrowed features use the same prepare/draw boundary as retained registrations.
/// This lets a resident scene keep its existing exclusive handle-editing API.
pub struct BorrowedFeature<'a> {
    pub declaration: FeatureDeclaration,
    pub prepared: &'a mut dyn PreparedFeature,
}

/// Views are borrowed for this encode only; plans never retain surface views.
#[derive(Clone, Debug)]
pub struct AttachmentView {
    resource: ResourceRef,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
}
impl AttachmentView {
    pub const fn resource(&self) -> ResourceRef {
        self.resource
    }
    pub fn texture(&self) -> &wgpu::Texture {
        self.view.texture()
    }
    /// The checked view's attachment format, not an inferred pipeline format.
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// The selected mip's extent, independent of the parent texture's base size.
    pub fn size(&self) -> [u32; 2] {
        [
            (self.texture().width() >> self.resource.mip).max(1),
            (self.texture().height() >> self.resource.mip).max(1),
        ]
    }
    /// Relabels a checked view without changing its selected mip or layer.
    pub fn with_resource(&self, resource: ResourceRef) -> Result<Self, FrameError> {
        if self.resource.mip != resource.mip || self.resource.layer != resource.layer {
            return Err(FrameError::AttachmentView(self.resource));
        }
        Ok(Self {
            resource,
            view: self.view.clone(),
            format: self.format,
        })
    }
    /// Creates the exact declared single-mip, single-layer view, avoiding caller/view drift.
    pub fn new(resource: ResourceRef, texture: &wgpu::Texture) -> Result<Self, FrameError> {
        if texture.dimension() != wgpu::TextureDimension::D2
            || resource.mip >= texture.mip_level_count()
            || resource.layer >= texture.depth_or_array_layers()
        {
            return Err(FrameError::AttachmentView(resource));
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(texture.format()),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: resource.mip,
            mip_level_count: Some(1),
            base_array_layer: resource.layer,
            array_layer_count: Some(1),
            ..Default::default()
        });
        Ok(Self {
            resource,
            view,
            format: texture.format(),
        })
    }
}

pub(crate) fn validate_views(
    passes: &[PassDeclaration],
    views: &[AttachmentView],
) -> Result<(), FrameError> {
    for (index, view) in views.iter().enumerate() {
        if views[..index].iter().any(|other| {
            other.resource == view.resource
                || (other.view.texture() == view.view.texture()
                    && other.resource.mip == view.resource.mip
                    && other.resource.layer == view.resource.layer)
        }) {
            return Err(FrameError::AttachmentHazard(view.resource));
        }
    }
    for attachment in passes
        .iter()
        .flat_map(|pass| pass.color.iter().chain(&pass.depth))
    {
        let view = views
            .iter()
            .find(|view| view.resource == attachment.resource)
            .ok_or(FrameError::AttachmentView(attachment.resource))?;
        let texture = view.view.texture();
        if view.format() != attachment.format
            || texture.sample_count() != attachment.samples
            || view.size() != attachment.size
            || !texture
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(FrameError::AttachmentView(attachment.resource));
        }
    }
    Ok(())
}

pub(crate) fn encode_pass(
    encoder: &mut wgpu::CommandEncoder,
    declaration: &PassDeclaration,
    views: &[AttachmentView],
    mut draw: impl FnMut(&mut wgpu::RenderPass<'_>),
) {
    let view = |resource| {
        &views
            .iter()
            .find(|view| view.resource == resource)
            .expect("validated attachment view")
            .view
    };
    let color = declaration
        .color
        .as_ref()
        .map(|a| wgpu::RenderPassColorAttachment {
            view: view(a.resource),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: match a.load {
                    AttachmentLoad::ClearColor(color) => wgpu::LoadOp::Clear(color),
                    AttachmentLoad::Load => wgpu::LoadOp::Load,
                    AttachmentLoad::ClearDepth(_) => unreachable!("validated color"),
                },
                store: a.store,
            },
        });
    let depth = declaration
        .depth
        .as_ref()
        .map(|a| wgpu::RenderPassDepthStencilAttachment {
            view: view(a.resource),
            stencil_ops: None,
            depth_ops: Some(wgpu::Operations {
                load: match a.load {
                    AttachmentLoad::ClearDepth(depth) => wgpu::LoadOp::Clear(depth),
                    AttachmentLoad::Load => wgpu::LoadOp::Load,
                    AttachmentLoad::ClearColor(_) => unreachable!("validated depth"),
                },
                store: a.store,
            }),
        });
    let colors = [color];
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("frame slot"),
        color_attachments: if declaration.color.is_some() {
            &colors
        } else {
            &[]
        },
        depth_stencil_attachment: depth,
        ..Default::default()
    });
    draw(&mut pass);
}

/// Complete borrowed geometry contract, validated before feature preparation.
pub struct FramePlan<'a> {
    pub passes: &'a [PassDeclaration],
    pub views: &'a [AttachmentView],
    pub requested: &'a [&'static str],
}
