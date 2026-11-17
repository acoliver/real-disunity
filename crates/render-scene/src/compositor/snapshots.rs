use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FrameConfig, FrameError, PassDeclaration, ResourceRef,
};

/// Plans the snapshots the declared reads ask for. `features` are the declarations of the
/// frame's own features and then those of its retained ones. `state` is the snapshot banks and
/// whether depth-tested overlay geometry reads the post-water depth.
pub(super) fn plan(
    device: &wgpu::Device,
    state: (&mut Option<[crate::water::snapshots::Snapshots; 3]>, bool),
    input: &(FrameConfig, crate::frame::FrameUniform, usize, bool),
    plan: (&mut Vec<PassDeclaration>, &mut Vec<AttachmentView>),
    features: (&[&FeatureDeclaration], &[FeatureDeclaration]),
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
        .copied()
        .chain(features.1.iter())
        .flat_map(|feature| &feature.passes)
        .flat_map(|pass| &pass.reads)
        .copied()
        .collect();
    let (snapshots, overlay_depth) = state;
    let (config, frame, slot, water) = *input;
    if overlay_depth {
        reads.push(ResourceRef::new("post-water depth"));
    }
    crate::water::planning::snapshot_feature(
        device,
        snapshots,
        (config, &frame, slot),
        plan,
        &reads,
        water,
    )
}
