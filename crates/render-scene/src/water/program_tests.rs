//! Claims about the water body path that need no byte comparison.

/// The water files the body path composes, with the snapshot of each. `water_source` joins them
/// to the lighting, post and shadow crates' text and to `shader/lighting.wgsl`,
/// `shader/camera_rays.wgsl` and one of `shader/shadows.wgsl` and `shader/unshadowed.wgsl`,
/// which `object/fixtures` freezes and `object::program_tests` pins.
///
/// A capture that compares the scene's water with a fixed path built from the snapshots means
/// what it says only while the live files equal them. A change to a live file fails here before
/// it can hide inside such a capture.
macro_rules! pairs {
    ($($name:literal),* $(,)?) => {
        [$((
            $name,
            include_str!(concat!($name, ".wgsl")),
            include_str!(concat!("fixtures/", $name, ".wgsl")),
        )),*]
    };
}

#[test]
fn the_live_water_wgsl_equals_the_frozen_snapshot() {
    let pairs = pairs![
        "ambient_visibility",
        "optics",
        "water",
        "screen",
        "terms",
        "shadow_lookup",
        "shadow_response",
        "shadow_visibility",
    ];
    let drifted: Vec<_> = pairs
        .iter()
        .filter(|(_, live, frozen)| live != frozen)
        .map(|(name, ..)| *name)
        .collect();
    assert!(
        drifted.is_empty(),
        "the live water WGSL differs from its frozen snapshot in water/fixtures for {drifted:?}. \
         When the change is intended, copy the live file over the snapshot \
         (`cp water/<name>.wgsl water/fixtures/<name>.wgsl` from crates/render-scene/src)."
    );
}

/// The frozen composition is the live one while the files are equal: the fixed path's source
/// for each of the two bases is `water_source` of that base, text for text. This pins the join
/// order and the two text edits of the shared files (`@binding(0)` to `@binding(3)` in the
/// lighting file, `@group(2)` to `@group(1)` in the shadow file), which the file-by-file test
/// above cannot see.
#[test]
fn the_frozen_composition_equals_the_live_one_for_both_bases() {
    for shadows in [false, true] {
        assert!(
            super::fixed::source(shadows) == crate::water::water_source(shadows),
            "shadows={shadows}: the frozen water composition differs from the live one"
        );
    }
}

/// The body program declares set 0 for itself, while the reactive mask still binds the fixed
/// groups of `input_layout`. The two must stay one table: a binding, a stage or a kind changed in
/// one and not the other would bind different inputs on the two paths.
#[test]
fn the_body_program_declares_set_zero_as_the_fixed_groups_do() {
    let fixed = crate::water::input_entries();
    let program = crate::water::program::bindings();
    assert_eq!(fixed.len(), program.len(), "both declare every binding");
    for entry in &fixed {
        let declared = program
            .iter()
            .find(|binding| binding.at.set == 0 && binding.at.binding == entry.binding)
            .unwrap_or_else(|| panic!("the program does not declare binding {}", entry.binding));
        assert_eq!(
            declared.visibility, entry.visibility,
            "binding {}: stages",
            entry.binding
        );
        assert!(
            declared.kind.matches(&entry.ty),
            "binding {}: the program declares {:?}, the fixed group {:?}",
            entry.binding,
            declared.kind,
            entry.ty
        );
    }
}

/// Water's own text reads every set-0 binding in every key the captures reach, shadowed and
/// not, so a draw that lends every input never meets the runtime's `UnusedImport` on the
/// production path. Only a substituted test text may read fewer, and only there does
/// `Programs::prepare` hold an input back.
#[test]
fn every_body_program_of_the_composed_text_reads_every_set_zero_input() {
    use worldrunner_render_program::binding::BindingSource;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let mut programs = crate::water::program::Programs::new(context.device(), context.submission());
    let declared = crate::water::program::bindings();
    // Default, terms, vertex seeds, endpoint, endpoint with seeds, shadow response, and the
    // weighted filter: the keys of the capture cases.
    for bits in [0, 19, 64 | 512, 64 | 256, 64 | 256 | 512, 1024, 1024 | 2048] {
        for shadows in [false, true] {
            let program = programs.program(([0, 0, 0, 0, bits], shadows)).unwrap();
            let slots = program.layout().slots();
            for binding in declared.iter().filter(|binding| binding.at.set == 0) {
                let BindingSource::Engine(semantic) = &binding.source else {
                    continue;
                };
                assert!(
                    slots
                        .iter()
                        .any(|slot| slot.source == BindingSource::Engine(semantic.clone())),
                    "options {bits} shadows {shadows}: the composed text does not read set 0 \
                     binding {} ({semantic:?})",
                    binding.at.binding
                );
            }
        }
    }
}
