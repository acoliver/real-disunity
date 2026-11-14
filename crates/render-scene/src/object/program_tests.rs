//! Claims about the object forward path that need no byte comparison.

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
