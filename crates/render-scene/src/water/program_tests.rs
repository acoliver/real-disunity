//! Claims about the water body path that need no byte comparison.

/// The four bits that change water's text, as `program::text` reads them: endpoint addressing,
/// vertex seeds, the shadow response and its weighted filter.
const ENDPOINT: u32 = super::super::endpoint::SPECIALIZATION;
const SEEDS: u32 = super::super::vertex_seeds::SPECIALIZATION;
const RESPONSE: u32 = super::super::shadow_response::SPECIALIZATION;
const FILTER: u32 = super::super::shadow_filter::SPECIALIZATION;
const TEXT_BITS: u32 = ENDPOINT | SEEDS | RESPONSE | FILTER;
/// Flow coordinates (`terms.rs`, bit 6), which every seeded body has.
const FLOW: u32 = 64;

/// Every reachable combination of the four text bits: endpoint addressing and vertex seeds (each
/// with flow coordinates), and the shadow response with or without the weighted filter (which a
/// response always carries). Plus the term bits 19, which only set override constants. A new
/// text bit still has to be added here by hand; the key-set test then counts it.
fn text_keys() -> Vec<u32> {
    let mut keys = vec![19];
    for seeds in [0, ENDPOINT, SEEDS, ENDPOINT | SEEDS] {
        let flow = if seeds == 0 { 0 } else { FLOW };
        for response in [0, RESPONSE, RESPONSE | FILTER] {
            keys.push(flow | seeds | response);
        }
    }
    keys
}

#[test]
fn the_text_keys_cover_every_reachable_combination_of_the_text_bits() {
    let keys = text_keys();
    assert_eq!(keys.len(), 13, "12 text combinations and the term bits");
    for key in &keys {
        assert!(
            key & FILTER == 0 || key & RESPONSE != 0,
            "{key}: the filter needs a response"
        );
        assert!(
            key & (ENDPOINT | SEEDS) == 0 || key & FLOW != 0,
            "{key}: seeds need flow coordinates"
        );
    }
    let distinct: std::collections::BTreeSet<_> = keys.iter().map(|key| key & TEXT_BITS).collect();
    assert_eq!(distinct.len(), 12, "every text combination appears");
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
    for bits in text_keys() {
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
