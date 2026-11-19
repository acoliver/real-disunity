use super::snapshots::{SnapshotFeature, Snapshots};
use crate::frame::FrameUniform;
use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FrameConfig, FrameError, PassDeclaration, PassEntry,
    PassSlot, ResourceRef,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SnapshotRequest {
    pub color: bool,
    pub depth: bool,
    pub final_depth: bool,
}

pub(crate) fn opaque_source<'a>(
    passes: &[PassDeclaration],
    views: &'a [AttachmentView],
    config: FrameConfig,
    depth: bool,
) -> Result<&'a AttachmentView, FrameError> {
    let opaque = passes
        .iter()
        .find(|pass| pass.slot == PassSlot::Opaque)
        .ok_or(FrameError::MissingPass(PassEntry::new(PassSlot::Opaque)))?;
    let attachment = if depth { &opaque.depth } else { &opaque.color }
        .as_ref()
        .ok_or(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::Opaque,
        )))?;
    let view = views
        .iter()
        .find(|view| view.resource() == attachment.resource)
        .ok_or(FrameError::AttachmentView(attachment.resource))?;
    let format = if depth && attachment.format == wgpu::TextureFormat::Depth32FloatStencil8 {
        attachment.format
    } else if depth {
        wgpu::TextureFormat::Depth32Float
    } else {
        config.format
    };
    if attachment.size != config.size
        || attachment.samples != config.samples
        || attachment.format != format
        || view.size() != config.size
        || view.texture().sample_count() != config.samples
        || view.format() != format
        || !view
            .texture()
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::Opaque,
        )));
    }
    Ok(view)
}

impl SnapshotRequest {
    pub(crate) const fn any(self) -> bool {
        self.color || self.depth || self.final_depth
    }
}

/// The snapshots the declared reads ask for. Water always asks for the final depth.
pub(crate) fn snapshot_request(reads: &[ResourceRef], water: bool) -> SnapshotRequest {
    SnapshotRequest {
        color: reads
            .iter()
            .any(|read| read.name == "opaque snapshot color"),
        depth: reads
            .iter()
            .any(|read| read.name == "opaque snapshot depth"),
        final_depth: water || reads.iter().any(|read| read.name == "post-water depth"),
    }
}

/// The Opaque pass's colour and depth views, as far as `request` snapshots them.
fn sources(
    plan: (&[PassDeclaration], &[AttachmentView]),
    config: FrameConfig,
    request: SnapshotRequest,
) -> Result<(Option<AttachmentView>, Option<AttachmentView>), FrameError> {
    if request.color && config.format != wgpu::TextureFormat::Rgba16Float {
        return Err(FrameError::UnsupportedEffect("snapshots require HDR"));
    }
    let (passes, views) = plan;
    let color = request
        .color
        .then(|| opaque_source(passes, views, config, false).cloned())
        .transpose()?;
    let depth = (request.depth || request.final_depth)
        .then(|| opaque_source(passes, views, config, true).cloned())
        .transpose()?;
    Ok((color, depth))
}

/// The executor of the frame slot's snapshots, which allocates the banks on first use.
fn executor(
    device: &wgpu::Device,
    slots: &mut Option<[Snapshots; 3]>,
    input: (FrameConfig, &FrameUniform, usize),
    sources: (Option<&AttachmentView>, Option<&AttachmentView>),
) -> SnapshotFeature {
    let (config, frame, slot) = input;
    let snapshots =
        slots.get_or_insert_with(|| std::array::from_fn(|_| Snapshots::new(device, config)));
    let snapshot = &snapshots[slot];
    SnapshotFeature {
        snapshots: snapshot.clone(),
        frame: *frame,
        color_source: sources
            .0
            .map(|view| snapshot.source_group(device, view, false)),
        depth_source: sources
            .1
            .map(|view| snapshot.source_group(device, view, true)),
    }
}

/// The executor of the snapshots a planned frame requested, from its planned passes and views.
///
/// # Errors
/// When the Opaque pass cannot be snapshotted as the request asks.
pub(crate) fn snapshot_executor(
    device: &wgpu::Device,
    slots: &mut Option<[Snapshots; 3]>,
    input: (FrameConfig, &FrameUniform, usize),
    plan: (&[PassDeclaration], &[AttachmentView]),
    request: SnapshotRequest,
) -> Result<Option<SnapshotFeature>, FrameError> {
    if !request.any() {
        return Ok(None);
    }
    let (color, depth) = sources(plan, input.0, request)?;
    Ok(Some(executor(
        device,
        slots,
        input,
        (color.as_ref(), depth.as_ref()),
    )))
}

/// Plans the snapshots the declared `reads` ask for: their passes and views, the declaration of
/// the feature that draws them and its executor.
///
/// # Errors
/// When the Opaque pass cannot be snapshotted as the request asks.
pub(crate) fn snapshot_feature(
    device: &wgpu::Device,
    slots: &mut Option<[Snapshots; 3]>,
    input: (FrameConfig, &FrameUniform, usize),
    plan: (&mut Vec<PassDeclaration>, &mut Vec<AttachmentView>),
    reads: &[ResourceRef],
    water: bool,
) -> Result<
    (
        SnapshotRequest,
        Option<(FeatureDeclaration, SnapshotFeature)>,
    ),
    FrameError,
> {
    let request = snapshot_request(reads, water);
    if !request.any() {
        return Ok((request, None));
    }
    let (passes, views) = plan;
    let (color, depth) = sources((passes, views), input.0, request)?;
    let feature = executor(device, slots, input, (color.as_ref(), depth.as_ref()));
    let declaration =
        feature
            .snapshots
            .schedule(passes, views, request, color.as_ref(), depth.as_ref())?;
    Ok((request, Some((declaration, feature))))
}
