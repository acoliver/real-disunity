//! What the seam refuses, by name, and before anything records.
use super::fixture::{Rig, SIZE, quad_scene};
use super::layers::{self, ARRAY, LayersGame};
use super::prepared;
use super::strip::{NAMES, StripGame, knobs};
use std::sync::Arc;
use worldrunner_render_post::FrameError;
use worldrunner_render_scene::compositor::GamePasses;

#[test]
fn a_program_for_other_attachments_than_its_pass_is_refused_by_name_before_anything_records() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs([0.0, -1.0, 0.8, 1.0], 0.2);
    knobs.lock().unwrap().wrong_signature = true;
    let game = Arc::new(StripGame {
        knobs: knobs.clone(),
    });
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(game);
    });
    let submitted = rig.context.submission().submitted_serial();
    let error = rig.render().unwrap_err();
    let FrameError::FeaturePreparation { feature, reason } = error else {
        panic!("expected a refusal by name, got {error:?}");
    };
    assert_eq!(feature, "strip");
    assert!(
        reason.contains("Game(BeforeOpaque)"),
        "names the pass: {reason}"
    );
    assert!(
        reason.contains("Rgba8UnormSrgb"),
        "names what the pass opens: {reason}"
    );
    assert!(
        reason.contains(
            "its draw targets TargetSignature { colors: [Some(Rgb10a2Unorm), Some(Rgba8Unorm),"
        ),
        "names what it prepared for: {reason}"
    );
    let knobs = knobs.lock().unwrap();
    assert!(knobs.counts.recorded.is_empty(), "nothing recorded");
    assert_eq!(
        rig.context.submission().submitted_serial(),
        submitted,
        "nothing submitted"
    );
}

#[test]
fn a_read_of_targets_no_earlier_pass_writes_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = knobs([0.0, -1.0, 0.8, 1.0], 0.2);
    knobs.lock().unwrap().skip_strip = true;
    let game = Arc::new(StripGame {
        knobs: knobs.clone(),
    });
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(game);
    });
    let error = rig.render().unwrap_err();
    assert!(
        matches!(error, FrameError::MissingProducer(resource) if resource.name == NAMES[0]),
        "{error:?}"
    );
    assert_eq!(
        knobs.lock().unwrap().counts.prepares,
        0,
        "refused before any feature prepared"
    );
}

#[test]
fn a_persistent_array_nothing_has_written_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = layers::knobs();
    let game = Arc::new(LayersGame {
        knobs: knobs.clone(),
    });
    let mut rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(game);
    });
    let error = rig.render().unwrap_err();
    assert!(
        matches!(error, FrameError::MissingProducer(resource) if resource.name == ARRAY),
        "{error:?}"
    );
    assert_eq!(knobs.lock().unwrap().counts.prepares, 0);
}

/// The refusal of the first frame of a block game with `fault`, and the game's knobs after it.
fn block_refusal(fault: prepared::Fault, compress: &[u32]) -> (FrameError, prepared::Shared) {
    let knobs = prepared::knobs(compress, &[]);
    knobs.lock().unwrap().fault = fault;
    let mut rig = prepared::rig(&knobs);
    let submitted = rig.context.submission().submitted_serial();
    let error = rig.render().unwrap_err();
    assert_eq!(
        rig.context.submission().submitted_serial(),
        submitted,
        "nothing submitted"
    );
    assert!(!rig.renderer.game_targets().is_written(prepared::BLOCKS, 0));
    (error, knobs)
}

#[test]
fn a_prepared_write_the_plan_did_not_declare_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = prepared::knobs(&[0], &[0]);
    let mut rig = prepared::rig(&knobs);
    prepared::assert_blocks(&rig.render().unwrap(), false, "frame 1");
    prepared::next(&knobs, &[], &[0]);
    knobs.lock().unwrap().fault = prepared::Fault::Unplanned;
    let submitted = rig.context.submission().submitted_serial();
    let error = rig.render().unwrap_err();
    assert!(
        matches!(error, FrameError::UnplannedWrite(resource) if resource == prepared::declaration().layer(0)),
        "{error:?}"
    );
    assert_eq!(
        rig.context.submission().submitted_serial(),
        submitted,
        "nothing submitted"
    );
}

#[test]
fn a_layer_staged_twice_in_one_preparation_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = block_refusal(prepared::Fault::Repeated, &[0]);
    assert!(
        matches!(error, FrameError::RepeatedPreparedWrite(resource) if resource == prepared::declaration().layer(0)),
        "{error:?}"
    );
    assert_eq!(knobs.lock().unwrap().transfers, 1, "the first write staged");
}

#[test]
fn a_planned_prepared_write_no_preparation_staged_refuses_the_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, _) = block_refusal(prepared::Fault::Unstaged, &[1]);
    assert!(
        matches!(error, FrameError::UnstagedWrite(resource) if resource == prepared::declaration().layer(1)),
        "{error:?}"
    );
}

#[test]
fn a_prepared_write_to_an_undeclared_target_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = block_refusal(prepared::Fault::UndeclaredPlan, &[0]);
    assert!(
        matches!(error, FrameError::AttachmentView(resource) if resource.name == prepared::MISSING),
        "{error:?}"
    );
    assert_eq!(knobs.lock().unwrap().prepares, 0, "refused at planning");
    let (error, knobs) = block_refusal(prepared::Fault::UndeclaredStage, &[]);
    assert!(
        matches!(error, FrameError::AttachmentView(resource) if resource.name == prepared::MISSING),
        "{error:?}"
    );
    assert_eq!(knobs.lock().unwrap().prepares, 1, "refused while staging");
}

#[test]
fn a_prepared_read_of_a_layer_nothing_wrote_is_refused_by_name_at_planning() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = block_refusal(prepared::Fault::PreparedRead, &[1]);
    assert!(
        matches!(error, FrameError::MissingProducer(resource) if resource == prepared::declaration().layer(0)),
        "{error:?}"
    );
    assert_eq!(knobs.lock().unwrap().prepares, 0);
}

#[test]
fn a_block_target_as_a_colour_output_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = block_refusal(prepared::Fault::ColorOutput, &[]);
    assert!(
        matches!(error, FrameError::AttachmentView(resource) if resource == prepared::declaration().layer(0)),
        "{error:?}"
    );
    assert_eq!(knobs.lock().unwrap().prepares, 0);
}

#[test]
fn staging_prepared_commands_after_preparation_is_refused() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = prepared::knobs(&[0, 1], &[0, 1]);
    knobs.lock().unwrap().fault = prepared::Fault::FinishDraws;
    let mut rig = prepared::rig(&knobs);
    prepared::assert_blocks(&rig.render().unwrap(), true, "the frame itself");
    let answer = knobs.lock().unwrap().finish_draws.clone();
    assert_eq!(answer, Some(Err(FrameError::PreparedOutsidePreparation)));
}

#[test]
fn a_transfer_into_another_layer_than_its_declared_write_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (error, knobs) = block_refusal(prepared::Fault::WrongLayer, &[0]);
    let target = prepared::declaration();
    assert_eq!(
        error,
        FrameError::PreparedWriteMismatch {
            label: "blocks transfer",
            declared: vec![target.layer(0)],
            written: vec![target.layer(1)],
        }
    );
    assert_eq!(knobs.lock().unwrap().transfers, 0, "nothing staged");
}
