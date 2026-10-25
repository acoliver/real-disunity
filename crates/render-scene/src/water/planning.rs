use super::snapshots::{SnapshotFeature, Snapshots};
use crate::frame::FrameUniform;
use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FrameConfig, FrameError, PassDeclaration, PassSlot,
    ResourceRef,
};

#[derive(Clone, Copy, Debug, Default)]
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
        .ok_or(FrameError::MissingSlot(PassSlot::Opaque))?;
    let attachment = if depth { &opaque.depth } else { &opaque.color }
        .as_ref()
        .ok_or(FrameError::AttachmentContract(PassSlot::Opaque))?;
    let view = views
        .iter()
        .find(|view| view.resource() == attachment.resource)
        .ok_or(FrameError::AttachmentView(attachment.resource))?;
    let format = if depth {
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
        return Err(FrameError::AttachmentContract(PassSlot::Opaque));
    }
    Ok(view)
}

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
    let request = SnapshotRequest {
        color: reads
            .iter()
            .any(|read| read.name == "opaque snapshot color"),
        depth: reads
            .iter()
            .any(|read| read.name == "opaque snapshot depth"),
        final_depth: water || reads.iter().any(|read| read.name == "post-water depth"),
    };
    if !request.color && !request.depth && !request.final_depth {
        return Ok((request, None));
    }
    let (config, frame, slot) = input;
    if config.format != wgpu::TextureFormat::Rgba16Float {
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
    let snapshots =
        slots.get_or_insert_with(|| std::array::from_fn(|_| Snapshots::new(device, config)));
    let snapshot = &snapshots[slot];
    let color_source = color
        .as_ref()
        .map(|view| snapshot.source_group(device, view, false));
    let depth_source = depth
        .as_ref()
        .map(|view| snapshot.source_group(device, view, true));
    let declaration = snapshot.schedule(passes, views, request, color.as_ref(), depth.as_ref())?;
    Ok((
        request,
        Some((
            declaration,
            SnapshotFeature {
                snapshots: snapshot.clone(),
                frame: *frame,
                color_source,
                depth_source,
            },
        )),
    ))
}
