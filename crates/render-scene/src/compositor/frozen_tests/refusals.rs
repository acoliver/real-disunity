//! The renderer's frozen entry refuses a plan that breaks an order the stock passes need, by
//! name, before any feature prepares, and it does so for the stock declarations too.
use super::support::{Counting, Feature, Rig, declaring};
use crate::renderer::{ScenePipelines, depth::DepthMode};
use worldrunner_render_post::{
    BorrowedFeature, FeatureDeclaration, FrameError, PassDeclaration, PassEntry, PassSlot,
};

/// Encodes the plan with one counting feature and reports the refusal and what prepared.
fn refusal(
    rig: &mut Rig,
    plan: (
        &[PassDeclaration],
        &[worldrunner_render_post::AttachmentView],
    ),
    declaration: FeatureDeclaration,
) -> (FrameError, u32) {
    let counting = Counting::default();
    let prepared = counting.prepared.clone();
    let mut feature = Feature(counting);
    let mut features = [BorrowedFeature {
        declaration,
        prepared: &mut feature,
    }];
    let error = rig
        .encode(plan, &mut features)
        .expect_err("the plan is refused");
    (error, prepared.get())
}

#[test]
fn an_entry_that_differs_from_the_stock_scene_declaration_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    // The stock scene feature declares an sRGB colour target, and the plan's Opaque entry
    // draws into the renderer's HDR scene format.
    let pipelines = ScenePipelines::new(rig.device(), wgpu::TextureFormat::Rgba8UnormSrgb, 1)
        .unwrap()
        .for_depth(DepthMode::Independent);
    let declaration = crate::compositor::SceneFeature::declaration(&pipelines);
    assert_ne!(rig.format(), pipelines.color_format);
    let (mut opaque, color) = rig.colour("scene color");
    let (depth, depth_view) = rig.depth("scene depth");
    opaque.depth = Some(depth);
    let (error, prepared) = refusal(&mut rig, (&[opaque], &[color, depth_view]), declaration);
    assert_eq!(
        error,
        FrameError::AttachmentContract(PassEntry::new(PassSlot::Opaque))
    );
    assert_eq!(prepared, 0);
}

#[test]
fn a_plan_in_the_stock_order_with_a_caller_pass_between_is_not_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let (first, first_view) = rig.colour("opaque target");
    let (middle, middle_view) = rig.colour("caller target");
    let mut middle = middle;
    middle.slot = PassSlot::Celestial;
    let (mut last, last_view) = rig.colour("reflection target");
    last.slot = PassSlot::ReflectionTrace;
    let entries = [first.entry(), middle.entry(), last.entry()];
    let counting = Counting::default();
    let draws = counting.draws.clone();
    let mut feature = Feature(counting);
    let mut features = [BorrowedFeature {
        declaration: declaring(&entries),
        prepared: &mut feature,
    }];
    rig.encode(
        (
            &[first, middle, last],
            &[first_view, middle_view, last_view],
        ),
        &mut features,
    )
    .unwrap();
    assert_eq!(*draws.borrow(), entries);
}
