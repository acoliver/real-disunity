//! Every context the crate builds can draw terrain on the program runtime.
//!
//! There is no other path: the forward target sets, the metadata prepass in both outputs and the
//! capture terms in every replay all record terrain through the runtime. Their captures prove it
//! for the draws they make, and this pins it for the contexts: each one's terrain program builds
//! for every surface source its device holds, so a context whose screen group the program does
//! not lend fails here instead of at its first draw.

use super::capture_tests::assert_recorded_by_runtime;
use super::program_tests::SURFACES;
use crate::compositor::terms::{CaptureTerm, pipelines_for};
use crate::device::GpuContext;
use crate::draw::terrain_paths;
use crate::dynamic::capture_tests::{CONFIGS, SIZE, build, context};
use crate::renderer::ScenePipelines;
use crate::scene_metadata::{MetadataOutput, SceneMetadata};

fn assert_prepares(name: &str, context: &GpuContext, pipelines: &ScenePipelines) {
    let terrain = pipelines.terrain_context();
    for surface in SURFACES {
        if surface.layered() && !pipelines.layered_terrain_supported() {
            continue;
        }
        terrain
            .family()
            .with((context.device(), context.submission()), |runtime| {
                terrain
                    .program(runtime, surface)
                    .unwrap_or_else(|error| panic!("{name} {surface:?}: {error}"));
            });
    }
}

#[test]
fn every_forward_context_draws_terrain_on_the_runtime() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for config in &CONFIGS {
        let built = build(context.device(), config);
        assert_prepares(config.name, &context, &built.pipelines);
    }
}

#[test]
fn the_metadata_prepass_draws_terrain_on_the_runtime_in_both_outputs() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let screen = SceneMetadata::new(device, [SIZE; 2], crate::ssr::neutral_ao()).unwrap();
    for (name, output) in [
        ("prepass-normal", MetadataOutput::Normal),
        ("prepass-merged", MetadataOutput::Merged),
    ] {
        let (prepass, _) = crate::scene_metadata::pipelines_from_sources(
            device,
            &screen.uniform,
            &crate::scene_metadata::sources(output),
        );
        assert_prepares(name, &context, &prepass);
    }
}

#[test]
fn every_capture_term_draws_terrain_on_the_runtime_in_every_replay() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let shadow = crate::shadows::water_layout(device);
    for term in [
        CaptureTerm::Albedo,
        CaptureTerm::SunVisibility,
        CaptureTerm::DirectDiffuse,
        CaptureTerm::AmbientDiffuse,
        CaptureTerm::EnvironmentSpecular,
        CaptureTerm::SunSpecular,
    ] {
        for (replay, layout, screen) in [
            ("plain", None, None),
            ("shadowed", Some(&shadow), None),
            ("ambient", None, Some(false)),
            ("reflection", None, Some(true)),
        ] {
            let pipelines = pipelines_for(device, term, layout, screen);
            assert_prepares(&format!("{term:?} {replay}"), &context, &pipelines);
        }
    }
}

/// A capture compares bytes with the frozen path, which a fixed-path draw reproduces, so the
/// recording itself is what the capture checks. This is the claim that a capture over which the
/// runtime recorded nothing is refused.
#[test]
#[should_panic(expected = "terrain was not recorded through the program runtime")]
fn a_capture_over_which_the_runtime_recorded_nothing_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    assert_recorded_by_runtime("nothing drawn", &built.pipelines, terrain_paths::recorded());
}
