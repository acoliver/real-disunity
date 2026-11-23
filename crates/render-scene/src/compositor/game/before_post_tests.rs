//! The place before post: where its passes sit among the stock passes, what they attach, and
//! the frames and combinations that refuse them.
use super::super::{ColorOutput, DepthAccess, GamePassSpec, GameTargets, TargetStore};
use super::{
    AttachmentLoad, COLOR, GamePlace, PassDeclaration, PassEntry, PassSlot, ResourceRef,
    attachment, opaque, prepass, raster, reconcile_loads, target,
};
use crate::compositor::forward_order::forward_order;
use std::sync::Arc;
use wgpu::CompareFunction;
use worldrunner_render_post::{FeatureDeclaration, FrameError, validate_frame};

const BEFORE_POST: GamePlace = GamePlace::BeforePost;
const SCENE_DEPTH: ResourceRef = ResourceRef::new("scene depth");

fn ordinary_targets() -> GameTargets {
    GameTargets::new(Arc::new(TargetStore::default()))
}

/// The stock screen-space pass after transparency, which loads the frame's colour and draws
/// the reflections over it.
fn screen_space() -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::ScreenSpaceAfterTransparent);
    pass.color = Some(attachment(
        "scene color",
        COLOR,
        AttachmentLoad::Load,
        false,
    ));
    pass
}

fn particles() -> GamePassSpec {
    GamePassSpec::new(BEFORE_POST, 0)
        .writing([ColorOutput::Frame])
        .with_depth(DepthAccess::ReadOnlySampled)
}

#[test]
fn a_pass_before_post_draws_over_every_colour_writer_and_tests_and_samples_the_depth() {
    let targets = ordinary_targets();
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0)), screen_space()];
    let mut views = Vec::new();
    let reflections =
        GamePassSpec::new(GamePlace::AfterTransparent, 0).writing([ColorOutput::Frame]);
    let declared = [
        targets
            .plan_pass(&particles(), &mut passes, &mut views)
            .unwrap(),
        targets
            .plan_pass(&reflections, &mut passes, &mut views)
            .unwrap(),
    ];
    assert!(views.is_empty(), "the frame's own attachments need no view");
    assert_eq!(
        declared[0].reads,
        [SCENE_DEPTH],
        "the sampled depth is a read"
    );
    forward_order(&mut passes);
    reconcile_loads(&mut passes);

    assert_eq!(
        passes
            .iter()
            .map(PassDeclaration::entry)
            .collect::<Vec<_>>(),
        [
            PassEntry::new(PassSlot::Opaque),
            reflections.entry(),
            PassEntry::new(PassSlot::ScreenSpaceAfterTransparent),
            particles().entry(),
        ]
    );
    let last = passes.last().unwrap();
    let color = last.color.as_ref().unwrap();
    assert_eq!(color.resource, ResourceRef::new("scene color"));
    assert_eq!(
        color.load,
        AttachmentLoad::Load,
        "it draws over the lit colour"
    );
    let depth = last.depth.as_ref().unwrap();
    assert_eq!(depth.resource, SCENE_DEPTH);
    assert!(depth.read_only);
    assert_eq!(depth.load, AttachmentLoad::Load);
    let features = [FeatureDeclaration {
        passes: declared.to_vec(),
        effects: Vec::new(),
    }];
    assert_eq!(
        validate_frame(&passes, &features, &[]),
        Ok(()),
        "sampling a depth the pass attaches read-only is no hazard"
    );

    let state = target(&passes, particles().entry()).render_state(raster());
    assert_eq!(
        state.colors.len(),
        1,
        "one colour state, for the frame colour the pass blends over"
    );
    let test = state.depth.unwrap();
    assert_eq!(test.test, CompareFunction::LessEqual);
    assert!(!test.write);
}

#[test]
fn a_frame_without_opaque_has_no_place_before_post() {
    let mut passes = vec![prepass()];
    assert_eq!(
        ordinary_targets().plan_pass(&particles(), &mut passes, &mut Vec::new()),
        Err(FrameError::MissingPass(PassEntry::new(PassSlot::Opaque)))
    );
    assert_eq!(passes.len(), 1, "nothing was planned");
}

#[test]
fn the_depth_prefix_feature_cannot_plan_before_post() {
    let targets = GameTargets::for_depth_prefix(Arc::new(TargetStore::default()));
    let mut passes = vec![prepass(), opaque(AttachmentLoad::Load)];
    assert!(matches!(
        targets.plan_pass(&particles(), &mut passes, &mut Vec::new()),
        Err(FrameError::UnsupportedEffect(_))
    ));
    assert_eq!(passes.len(), 2, "nothing was planned");
}

#[test]
fn a_pass_that_writes_the_frame_depth_cannot_sample_it() {
    let targets = ordinary_targets();
    let writer = GamePassSpec::new(GamePlace::BeforeOpaque, 0)
        .writing([ColorOutput::Frame])
        .with_depth(DepthAccess::ReadWrite)
        .sampling_frame([SCENE_DEPTH]);
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    assert_eq!(
        targets.plan_pass(&writer, &mut passes, &mut Vec::new()),
        Err(FrameError::AttachmentHazard(SCENE_DEPTH))
    );
    assert_eq!(passes.len(), 1, "nothing was planned");

    let tester = GamePassSpec {
        depth: Some(DepthAccess::ReadOnly),
        ..writer
    };
    let planned = targets
        .plan_pass(&tester, &mut passes, &mut Vec::new())
        .unwrap();
    assert_eq!(
        planned.reads,
        [SCENE_DEPTH],
        "a read-only pass may name the depth among its frame reads"
    );
}

#[test]
fn only_a_pass_planned_to_sample_the_depth_reads_it() {
    let targets = ordinary_targets();
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    let tester = GamePassSpec::new(BEFORE_POST, 0)
        .writing([ColorOutput::Frame])
        .with_depth(DepthAccess::ReadOnly);
    let planned = targets
        .plan_pass(&tester, &mut passes, &mut Vec::new())
        .unwrap();
    assert!(planned.reads.is_empty());
    assert!(passes[1].depth.as_ref().unwrap().read_only);
}
