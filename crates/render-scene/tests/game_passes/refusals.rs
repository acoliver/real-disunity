//! What the seam refuses, by name, and before anything records.
use super::fixture::{Rig, SIZE, quad_scene};
use super::layers::{self, ARRAY, LayersGame};
use super::strip::{NAMES, StripGame, knobs};
use std::sync::Arc;
use worldrunner_render_scene::{compositor::GamePasses, executor::FrameError};

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
