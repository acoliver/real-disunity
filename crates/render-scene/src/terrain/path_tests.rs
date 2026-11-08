//! Which path each context draws terrain through.
//!
//! Every forward context records terrain on the program runtime, which the captures prove for
//! the target sets they draw. The metadata prepass and the capture terms keep the fixed
//! pipelines until they move (`with_fixed_terrain`), and the prepass context is one the
//! runtime would refuse anyway: its group three is a camera uniform, not a screen group a
//! forward context makes. This pins both facts, so moving a pass is a visible change here and a
//! pass cannot leave the runtime unnoticed.

use super::capture_tests::{Case, Rig, Source, Target, capture, scenes};
use crate::dynamic::capture_tests::{CONFIGS, SIZE, build, context};
use crate::scene_metadata::{MetadataOutput, SceneMetadata};

#[test]
fn every_forward_context_draws_terrain_on_the_runtime() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for config in &CONFIGS {
        let built = build(context.device(), config);
        assert!(
            built.pipelines.terrain_on_runtime(),
            "{}: forward terrain is not on the program runtime",
            config.name
        );
        assert!(!built.pipelines.fixed_terrain_built(), "{}", config.name);
    }
}

#[test]
fn the_metadata_prepass_keeps_the_fixed_terrain_pipelines_until_it_moves() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let screen = SceneMetadata::new(device, [SIZE; 2], crate::ssr::neutral_ao()).unwrap();
    let (prepass, _) = crate::scene_metadata::pipelines_from_sources(
        device,
        &screen.uniform,
        &crate::scene_metadata::sources(MetadataOutput::Merged),
    );
    assert!(
        !prepass.terrain_on_runtime(),
        "the prepass terrain moved onto the runtime: cover it with a runtime capture and \
         remove this test"
    );
    assert!(
        !prepass.terrain_context().on_runtime(),
        "the prepass screen group is not one a forward context makes"
    );
}

/// The captures compare bytes with the frozen path, which a fixed-path draw reproduces. This is
/// the claim that a capture of such a draw is refused anyway.
#[test]
#[should_panic(expected = "terrain was not recorded through the program runtime")]
fn a_capture_of_terrain_drawn_by_the_fixed_pipelines_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let config = &CONFIGS[2];
    let built = build(context.device(), config);
    let fixed = built.pipelines.clone().with_fixed_terrain();
    let rig = Rig::new(
        &context,
        &fixed,
        &scenes::plain(),
        (Source::Plain, scenes::frame(scenes::pose())),
    );
    capture(
        &rig,
        &Case {
            name: config.name,
            source: Source::Plain.name(),
            target: Target {
                formats: &built.formats,
                samples: config.samples,
            },
            pipelines: &fixed,
            screen: built.screen.as_ref(),
            shadow: None,
        },
    );
}
