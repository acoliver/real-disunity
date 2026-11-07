use super::overlay;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameConfig, FrameError, PassDeclaration,
};
pub(super) fn plan(
    device: &wgpu::Device,
    state: (
        &mut Option<[crate::water::snapshots::Snapshots; 3]>,
        &mut overlay::OverlayStage,
    ),
    input: &(FrameConfig, crate::frame::FrameUniform, usize, bool),
    plan: (&mut Vec<PassDeclaration>, &mut Vec<AttachmentView>),
    features: (&[BorrowedFeature<'_>], &[FeatureDeclaration]),
) -> Result<
    (
        crate::water::planning::SnapshotRequest,
        Option<(FeatureDeclaration, crate::water::snapshots::SnapshotFeature)>,
    ),
    FrameError,
> {
    let mut reads: Vec<_> = features
        .0
        .iter()
        .map(|feature| &feature.declaration)
        .chain(features.1.iter())
        .flat_map(|feature| &feature.passes)
        .flat_map(|pass| &pass.reads)
        .copied()
        .collect();
    let (snapshots, overlay) = state;
    let (config, frame, slot, water) = *input;
    if overlay.needs_depth() {
        reads.push(worldrunner_render_post::ResourceRef::new(
            "post-water depth",
        ));
    }
    let result = crate::water::planning::snapshot_feature(
        device,
        snapshots,
        (config, &frame, slot),
        plan,
        &reads,
        water,
    )?;
    overlay.prepare_depth(device, snapshots.as_ref().map(|s| &s[slot]), &frame);
    Ok(result)
}
