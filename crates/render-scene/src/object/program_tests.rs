//! Claims about the object forward path that need no byte comparison.

/// The shader files the object forward path reads, with the snapshot of each. The object
/// forward path is the 32 `PipelineKey` variants of `vs_main` with the four fragment entry points
/// of `fs_opaque`, `fs_masked`, `fs_relief_opaque` and `fs_relief_masked`, and these files hold
/// them together with the lighting, decal, impostor, foliage and shadow code they call.
///
/// The captures in `object::capture_tests` compare the scene's output with the output of the
/// frozen path built from the snapshots. A change to a live file is then a change to what the
/// captures mean, and it fails here before it can hide inside a capture.
macro_rules! pairs {
    ($($name:literal => $live:expr),* $(,)?) => {
        [$((
            $name,
            $live,
            include_str!(concat!("fixtures/", $name, ".wgsl")),
        )),*]
    };
}

#[test]
fn the_live_object_wgsl_equals_the_frozen_snapshot() {
    let pairs = pairs! {
        "lighting_crate" => worldrunner_render_lighting::lighting_wgsl!(),
        "post_crate" => worldrunner_render_post::post_wgsl!(),
        "shadow_crate" => worldrunner_render_shadow::shadow_wgsl!(),
        "lighting" => include_str!("../shader/lighting.wgsl"),
        "ambient_visibility" => include_str!("../shader/ambient_visibility.wgsl"),
        "decals" => include_str!("../shader/decals.wgsl"),
        "decals_sample" => include_str!("../shader/decals_sample.wgsl"),
        "camera_rays" => include_str!("../shader/camera_rays.wgsl"),
        "unshadowed" => include_str!("../shader/unshadowed.wgsl"),
        "shadows" => include_str!("../shader/shadows.wgsl"),
        "geometry" => include_str!("../shader/geometry.wgsl"),
        "foliage" => include_str!("../shader/foliage.wgsl"),
        "impostor" => include_str!("../shader/impostor.wgsl"),
        "surface_sample" => include_str!("../shader/surface_sample.wgsl"),
        "impostor_end" => include_str!("../shader/impostor_end.wgsl"),
        "normal_forward" => include_str!("../shader/normal_forward.wgsl"),
        "normal_sample" => include_str!("../shader/normal_sample.wgsl"),
        "environment" => include_str!("../shader/environment.wgsl"),
        "forward_cards" => include_str!("../shader/forward_cards.wgsl"),
    };
    let drifted: Vec<_> = pairs
        .iter()
        .filter(|(_, live, frozen)| live != frozen)
        .map(|(name, ..)| *name)
        .collect();
    assert!(
        drifted.is_empty(),
        "the live object WGSL differs from its frozen snapshot in object/fixtures for {drifted:?}. \
         When the change is intended (another branch changes fs_masked, for one), copy the live \
         WGSL into object/fixtures so the snapshot matches it again: `cp shader/<name>.wgsl \
         object/fixtures/<name>.wgsl` from crates/render-scene/src, and for the three \
         *_crate files write the text the lighting, post and shadow crates' macros expand to. \
         Then rerun the captures: they compare the scene against the frozen path built from the \
         snapshots, and a snapshot that is not updated shows as a capture difference."
    );
    assert_eq!(
        crate::renderer::legacy_objects::compose(crate::renderer::legacy_objects::Recipe::default()),
        crate::shader::ENVIRONMENT_WGSL,
        "the frozen environment source is not the live one"
    );
}

/// Objects recorded through the program runtime, as the design's `draws_prepared` assertion for
/// slice 2: a frame of objects prepares at least one draw on the scene's runtime.
///
/// The scene still draws objects with the fixed pipelines, so no runtime sees them and this
/// reads zero. The assertion goes live in commit 2, where `ScenePipelines` carries the object
/// runtime and this helper reads its `RuntimeStats::draws_prepared`.
fn object_draws_prepared(_pipelines: &crate::renderer::ScenePipelines) -> u64 {
    0
}

#[test]
#[ignore = "red until slice 2 commit 2: the scene still records objects with the fixed pipelines"]
fn objects_are_recorded_through_the_program_runtime() {
    use super::capture_tests::{Rig, Target, render, scenes};
    use crate::dynamic::capture_tests::{CONFIGS, build, context};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let mut rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::scene(&scenes::base_specs()),
    );
    render(
        &context,
        Target {
            formats: &built.formats,
            samples: 1,
        },
        &mut rig.gpu,
        |pass, gpu| {
            gpu.draw_color(pass, &built.pipelines, None);
        },
    );
    assert!(
        object_draws_prepared(&built.pipelines) > 0,
        "objects were not recorded through the program runtime"
    );
}
