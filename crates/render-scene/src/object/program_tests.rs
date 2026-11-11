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
        "lighting_crate" => worldrunner_render_lighting::lighting_wgsl!()
            .strip_suffix("\n\n")
            .expect("the lighting crate's text ends in two blank lines, left out of the snapshot"),
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

/// What the program runtime has built for the objects: the pipelines and the prepared sets of
/// their materials.
///
/// The design reads `RuntimeStats::draws_prepared` here, but that counts one-shot prepared
/// draws, which `prepare_imported_draw` and materials make. The object path prepares pipelines
/// and sets and records geometries through them, so those are what a frame of objects adds.
fn object_programs_built(pipelines: &crate::renderer::ScenePipelines) -> (u64, u64) {
    let stats = pipelines.object_runtime_stats();
    (stats.pipelines_built, stats.sets_prepared)
}

/// Upload prepares material sets without building pipelines; the first object frame
/// builds pipelines on the program runtime. `cost_tests::through_runtime` tallies
/// commands separately because `draw_color` requires a concrete wgpu render pass.
#[test]
fn objects_are_recorded_through_the_program_runtime() {
    use super::capture_tests::{Rig, Target, render, scenes};
    use crate::dynamic::capture_tests::{CONFIGS, build, context};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    assert_eq!(
        object_programs_built(&built.pipelines),
        (0, 0),
        "nothing is built before a scene is uploaded"
    );
    let mut rig = Rig::new(
        &context,
        &built.pipelines,
        &scenes::scene(&scenes::base_specs()),
    );
    let (pipelines, sets) = object_programs_built(&built.pipelines);
    assert_eq!(pipelines, 0, "scene upload builds no object pipelines");
    assert!(sets > 0, "uploading the materials prepares their sets");
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
    let (pipelines, _) = object_programs_built(&built.pipelines);
    assert!(
        pipelines > 0,
        "objects were not recorded through the program runtime"
    );
}

/// Every key's program registers against the declared layouts of every forward context, and the
/// set-1 bindings the entry points derive are the eleven the material declares, so one prepared
/// set per material serves every key.
#[test]
fn object_programs_register_for_every_key_in_every_context_with_one_material_layout() {
    use crate::dynamic::capture_tests::{CONFIGS, build, context, supported};
    use crate::pipeline::PipelineKey;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for config in CONFIGS.iter().filter(|config| supported(&context, config)) {
        let built = build(context.device(), config);
        let objects = built.pipelines.object_context();
        objects
            .family()
            .with_objects((context.device(), context.submission()), |runtime| {
                let mut layout = None;
                for key in PipelineKey::ALL {
                    let program = objects.program(runtime, key).unwrap_or_else(|error| {
                        panic!("{} {key:?}: {error}", config.name);
                    });
                    let bindings: Vec<u32> = program
                        .layout()
                        .slots()
                        .iter()
                        .filter(|slot| slot.set == 1)
                        .map(|slot| slot.binding)
                        .collect();
                    assert_eq!(
                        bindings,
                        (0..=10).collect::<Vec<_>>(),
                        "{} {key:?}: the set-1 bindings the entry points use",
                        config.name
                    );
                    let found = program.set_layout(1).expect("set 1 has bindings").clone();
                    assert!(
                        layout.get_or_insert_with(|| found.clone()) == &found,
                        "{} {key:?}: set 1 has another layout than key 0's",
                        config.name
                    );
                }
            });
    }
}

/// A key's features are the five axes of its pipeline, so 32 keys are 32 feature sets and the
/// fragment entry point follows from the key as the fixed pipelines chose it.
#[test]
fn the_features_of_a_key_name_its_axes_and_are_distinct() {
    use crate::pipeline::PipelineKey;
    use std::collections::BTreeSet;
    let sets: BTreeSet<_> = PipelineKey::ALL.iter().map(|key| key.features()).collect();
    assert_eq!(sets.len(), 32);
    for key in PipelineKey::ALL {
        let features = key.features();
        let named = |name: &str| {
            features.contains(&worldrunner_render_program::variant::FeatureId::new(name))
        };
        assert_eq!(named("MASKED"), key.masked);
        assert_eq!(named("DOUBLE_SIDED"), key.double_sided);
        assert_eq!(named("RELIEF"), key.relief);
        assert_eq!(named("IMPOSTOR"), key.sampling.is_impostor());
        assert_eq!(
            named("COVERAGE_MULTISAMPLE"),
            key.coverage == crate::pipeline::MaskCoverage::Multisample
        );
        let entry = key.fragment_entry();
        assert_eq!(entry.contains("relief"), key.relief);
        assert_eq!(entry.ends_with("masked"), key.masked);
    }
}
