//! The rules of the game's passes after the depth prepass: which feature plans them, which
//! attachments they take, and when their depth is the frame's.
use super::super::{
    ColorOutput, DepthAccess, GamePassSpec, GameTargets, TargetStore, prepass_writes_frame_depth,
};
use super::{AttachmentLoad, GamePlace, PassEntry, PassSlot, ResourceRef, opaque, prepass, raster};
use std::sync::Arc;
use wgpu::CompareFunction;
use worldrunner_render_post::FrameError;

const AFTER_PREPASS: GamePlace = GamePlace::AfterDepthPrepass;

fn normals() -> ColorOutput {
    ColorOutput::Prepass(ResourceRef::new("prepass normals"))
}

fn prefix_targets() -> GameTargets {
    GameTargets::for_depth_prefix(Arc::new(TargetStore::default()))
}

fn ordinary_targets() -> GameTargets {
    GameTargets::new(Arc::new(TargetStore::default()))
}

fn plan(
    targets: &GameTargets,
    spec: &GamePassSpec,
) -> Result<Vec<worldrunner_render_post::PassDeclaration>, FrameError> {
    let mut passes = vec![prepass(), opaque(AttachmentLoad::Load)];
    targets.plan_pass(spec, &mut passes, &mut Vec::new())?;
    Ok(passes)
}

#[test]
fn a_pass_after_the_prepass_loads_and_writes_the_prepass_normals_and_depth() {
    let spec = GamePassSpec::new(AFTER_PREPASS, 0)
        .writing([normals()])
        .with_depth(DepthAccess::ReadWrite);
    let passes = plan(&prefix_targets(), &spec).unwrap();
    let game = &passes[2];
    let color = game.color.as_ref().unwrap();
    let source = passes[0].color.as_ref().unwrap();
    assert_eq!(color.resource, source.resource);
    assert_eq!(
        (color.format, color.size, color.samples),
        (source.format, source.size, source.samples)
    );
    assert_eq!(color.load, AttachmentLoad::Load);
    assert_eq!(color.store, wgpu::StoreOp::Store);
    let depth = game.depth.as_ref().unwrap();
    assert_eq!(depth.resource, passes[0].depth.as_ref().unwrap().resource);
    assert_eq!(depth.load, AttachmentLoad::Load);
    assert!(!depth.read_only);
    let state = super::target(&passes, spec.entry()).render_state(raster());
    let test = state.depth.unwrap();
    assert_eq!(test.test, CompareFunction::LessEqual);
    assert!(test.write);
    assert!(
        state.stencil.is_none(),
        "only the stock prepass marks reuse"
    );
}

#[test]
fn a_prepass_product_the_frame_lacks_is_refused_by_its_name() {
    let material = ResourceRef::new("reflection material");
    let spec = GamePassSpec::new(AFTER_PREPASS, 0).writing([ColorOutput::Prepass(material)]);
    assert_eq!(
        plan(&prefix_targets(), &spec),
        Err(FrameError::AttachmentView(material))
    );
}

#[test]
fn a_frame_without_a_depth_prepass_refuses_the_place() {
    let spec = GamePassSpec::new(AFTER_PREPASS, 0).with_depth(DepthAccess::ReadWrite);
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    assert_eq!(
        prefix_targets().plan_pass(&spec, &mut passes, &mut Vec::new()),
        Err(FrameError::MissingPass(PassEntry::new(
            PassSlot::DepthNormalPrepass
        )))
    );
}

#[test]
fn only_the_depth_prefix_feature_plans_the_place_and_only_that_place() {
    let after = GamePassSpec::new(AFTER_PREPASS, 0).with_depth(DepthAccess::ReadWrite);
    assert!(matches!(
        plan(&ordinary_targets(), &after),
        Err(FrameError::UnsupportedEffect(_))
    ));
    let before = GamePassSpec::new(GamePlace::BeforeOpaque, 0).with_depth(DepthAccess::ReadWrite);
    assert!(matches!(
        plan(&prefix_targets(), &before),
        Err(FrameError::UnsupportedEffect(_))
    ));
    assert!(plan(&ordinary_targets(), &before).is_ok());
}

#[test]
fn prepass_products_are_written_only_after_the_prepass_and_the_colour_only_later() {
    let products_late = GamePassSpec::new(GamePlace::BeforeOpaque, 0).writing([normals()]);
    assert!(matches!(
        plan(&ordinary_targets(), &products_late),
        Err(FrameError::UnsupportedEffect(_))
    ));
    let colour_early = GamePassSpec::new(AFTER_PREPASS, 0).writing([ColorOutput::Frame]);
    assert!(matches!(
        plan(&prefix_targets(), &colour_early),
        Err(FrameError::UnsupportedEffect(_))
    ));
}

#[test]
fn the_prepass_writes_the_frame_depth_when_opaque_draws_with_it_reading_or_writing() {
    let shared = [prepass(), opaque(AttachmentLoad::Load)];
    assert!(prepass_writes_frame_depth(&shared));
    let mut testing = opaque(AttachmentLoad::Load);
    testing.depth.as_mut().unwrap().read_only = true;
    assert!(
        prepass_writes_frame_depth(&[prepass(), testing]),
        "an Opaque that only tests the reused prepass depth draws with it"
    );
    let mut own = prepass();
    own.depth.as_mut().unwrap().resource = ResourceRef::new("prepass depth");
    assert!(!prepass_writes_frame_depth(&[
        own,
        opaque(AttachmentLoad::ClearDepth(1.0))
    ]));
    assert!(!prepass_writes_frame_depth(&[opaque(
        AttachmentLoad::ClearDepth(1.0)
    )]));
}

/// A game pass after the prepass that writes one layer of a declared target, as the planner
/// declares it before the prefix is assembled: clearing the layer, loading the prepass depth.
fn target_writer(occurrence: u32) -> worldrunner_render_post::PassDeclaration {
    let mut pass = super::game(AFTER_PREPASS, occurrence);
    pass.color = Some(super::attachment(
        "game target",
        wgpu::TextureFormat::Rgba8Unorm,
        super::clear_color(),
        false,
    ));
    pass.depth = Some(super::depth(AttachmentLoad::Load, false));
    pass
}

#[test]
fn a_later_prefix_game_pass_loads_a_target_an_earlier_one_wrote() {
    // The prefix's own passes, then the game's in planning order, which is not forward order.
    let mut passes = vec![prepass(), target_writer(1), target_writer(0)];
    super::super::place_after_prepass(&mut passes, 1).unwrap();
    let entries: Vec<_> = passes
        .iter()
        .map(worldrunner_render_post::PassDeclaration::entry)
        .collect();
    assert_eq!(
        entries,
        [
            PassEntry::new(PassSlot::DepthNormalPrepass),
            PassEntry::nth(PassSlot::Game(AFTER_PREPASS), 0),
            PassEntry::nth(PassSlot::Game(AFTER_PREPASS), 1),
        ]
    );
    assert_eq!(
        worldrunner_render_post::validate_frame(&passes, &[], &[]),
        Ok(())
    );
    let loads = super::loads(&passes);
    assert!(
        matches!(loads[1][0], AttachmentLoad::ClearColor(_)),
        "the first writer clears the target"
    );
    assert_eq!(
        loads[2],
        [AttachmentLoad::Load, AttachmentLoad::Load],
        "the second writer keeps what the first wrote"
    );
}

#[test]
fn prefix_game_passes_without_a_prepass_to_follow_are_refused_by_name() {
    // A feature that declares its own passes can place them without `plan_pass`, which refuses
    // the place when the list has no prepass.
    let mut passes = vec![opaque(AttachmentLoad::ClearDepth(1.0)), target_writer(0)];
    let planned = passes.clone();
    assert_eq!(
        super::super::place_after_prepass(&mut passes, 1),
        Err(FrameError::MissingPass(PassEntry::new(
            PassSlot::DepthNormalPrepass
        )))
    );
    assert_eq!(
        passes, planned,
        "a refused prefix is left as it was planned"
    );
    let mut stock_only = vec![opaque(AttachmentLoad::ClearDepth(1.0))];
    assert_eq!(
        super::super::place_after_prepass(&mut stock_only, 1),
        Ok(()),
        "a prefix without game passes needs no prepass"
    );
}
