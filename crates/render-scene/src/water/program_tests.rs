//! Claims about the water body path that need no byte comparison.

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
