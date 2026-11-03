//! Stage modules of one program variant, prepared together. The modules are built in
//! `support/stage_modules.rs`; `unity-import` checks that naga accepts them once prepared.

#[path = "support/stage_modules.rs"]
mod stage_modules;

use stage_modules::{
    FRAGMENT, Kind, Res, VERTEX, Variant, different_samplers, every_kind, fragment_only_split, op,
    place, res, sampler_places, split_and_plain_image, stage, two_sets,
};
use worldrunner_unity_serialized::spirv_prep::{
    PrepareError, PreparedModule, ResourceBinding, prepare, prepare_stages,
};

#[test]
fn modules_prepared_one_at_a_time_number_a_shared_binding_differently() {
    // Only the fragment stage declares a combined sampler, at binding 1 between two buffers.
    let [vertex, fragment] = <[_; 2]>::try_from(fragment_only_split()).unwrap();
    let vertex = prepare(&vertex).unwrap();
    let fragment = prepare(&fragment).unwrap();
    assert_eq!(place(&vertex.words, 1), (0, 2));
    assert_eq!(place(&fragment.words, 2), (0, 3));
    assert_eq!(sampler_places(&fragment.words), [(0, 2)]);
}

#[test]
fn the_split_puts_each_sampler_after_its_image_and_keeps_gaps() {
    let [module] = <[_; 1]>::try_from(every_kind()).unwrap();
    let prepared = prepare(&module).unwrap();
    let words = &prepared.words;
    let placed: Vec<_> = (0..5).map(|n| place(words, n)).collect();
    assert_eq!(placed, [(0, 0), (0, 3), (0, 4), (0, 5), (0, 7)]);
    assert_eq!(sampler_places(words), [(0, 1), (0, 8)]);
}

#[test]
fn each_descriptor_set_is_numbered_on_its_own() {
    let module = stage(
        FRAGMENT,
        &[
            res(0, Kind::Combined),
            Res {
                set: 1,
                binding: 1,
                kind: Kind::Uniform,
            },
        ],
    );
    let prepared = prepare(&module).unwrap();
    assert_eq!(place(&prepared.words, 1), (1, 1));
}

fn binding(unity_binding: u32, binding: u32, sampler_binding: Option<u32>) -> ResourceBinding {
    ResourceBinding {
        set: 0,
        unity_binding,
        binding,
        sampler_binding,
    }
}

/// Prepares `stages` together.
fn prepare_together(stages: &[&[u32]]) -> Vec<PreparedModule> {
    let prepared = prepare_stages(stages).unwrap();
    assert_eq!(prepared.modules.len(), stages.len());
    prepared.modules
}

/// The modules of a two-stage variant prepared together.
fn prepare_pair(variant: &Variant) -> [PreparedModule; 2] {
    let stages: Vec<&[u32]> = variant.iter().map(Vec::as_slice).collect();
    <[_; 2]>::try_from(prepare_together(&stages)).unwrap()
}

#[test]
fn a_sampler_only_the_fragment_splits_moves_the_vertex_buffers_too() {
    let [vertex, fragment] = prepare_pair(&fragment_only_split());
    let table = [
        binding(0, 0, None),
        binding(1, 1, Some(2)),
        binding(2, 3, None),
    ];
    assert_eq!(vertex.bindings, table);
    assert_eq!(fragment.bindings, table);
    // The stages decorate the shared Unity bindings alike, and the vertex stage keeps the
    // slots of the image and sampler it does not declare.
    assert_eq!(place(&vertex.words, 0), (0, 0));
    assert_eq!(place(&vertex.words, 1), (0, 3));
    assert_eq!(place(&fragment.words, 0), (0, 0));
    assert_eq!(place(&fragment.words, 1), (0, 1));
    assert_eq!(place(&fragment.words, 2), (0, 3));
    assert_eq!(sampler_places(&vertex.words), []);
    assert_eq!(sampler_places(&fragment.words), [(0, 2)]);
}

#[test]
fn stages_that_split_different_samplers_share_the_slots_of_both() {
    let [vertex, fragment] = prepare_pair(&different_samplers());
    let table = [
        binding(0, 0, Some(1)),
        binding(1, 2, None),
        binding(2, 3, Some(4)),
        binding(3, 5, None),
    ];
    assert_eq!(vertex.bindings, table);
    assert_eq!(fragment.bindings, table);
    let vertex_places: Vec<_> = (0..3).map(|n| place(&vertex.words, n)).collect();
    assert_eq!(vertex_places, [(0, 0), (0, 2), (0, 5)]);
    assert_eq!(sampler_places(&vertex.words), [(0, 1)]);
    let fragment_places: Vec<_> = (0..3).map(|n| place(&fragment.words, n)).collect();
    assert_eq!(fragment_places, [(0, 2), (0, 3), (0, 5)]);
    assert_eq!(sampler_places(&fragment.words), [(0, 4)]);
}

#[test]
fn a_binding_split_in_one_stage_and_a_plain_image_in_the_other_keeps_the_sampler_slot() {
    let [vertex, fragment] = prepare_pair(&split_and_plain_image());
    let table = [binding(0, 0, Some(1)), binding(1, 2, None)];
    assert_eq!(vertex.bindings, table);
    assert_eq!(fragment.bindings, table);
    assert_eq!(place(&fragment.words, 0), (0, 0));
    assert_eq!(place(&fragment.words, 1), (0, 2));
    assert_eq!(sampler_places(&fragment.words), []);
}

#[test]
fn descriptor_sets_are_planned_separately_across_stages() {
    let [vertex, fragment] = prepare_pair(&two_sets());
    assert_eq!(vertex.bindings, fragment.bindings);
    assert_eq!(place(&vertex.words, 0), (1, 0));
    assert_eq!(place(&vertex.words, 1), (0, 0));
    assert_eq!(place(&fragment.words, 0), (0, 1));
    assert_eq!(sampler_places(&fragment.words), [(0, 2)]);
    // Set 1 has no split sampler, so its bindings stay where Unity put them.
    assert_eq!(place(&fragment.words, 1), (1, 1));
}

#[test]
fn one_stage_is_prepared_as_prepare_does() {
    let [module] = <[_; 1]>::try_from(every_kind()).unwrap();
    let together = prepare_together(&[&module]);
    assert_eq!(together, [prepare(&module).unwrap()]);
}

#[test]
fn preparation_is_deterministic() {
    let vertex = stage(VERTEX, &[res(0, Kind::Combined), res(1, Kind::Combined)]);
    let fragment = stage(FRAGMENT, &[res(1, Kind::Combined), res(2, Kind::Combined)]);
    let first = prepare_stages(&[&vertex, &fragment]).unwrap();
    for _ in 0..16 {
        assert_eq!(prepare_stages(&[&vertex, &fragment]).unwrap(), first);
    }
}

#[test]
fn no_stages_prepare_to_no_modules() {
    assert!(prepare_stages(&[]).unwrap().modules.is_empty());
}

#[test]
fn a_unity_binding_used_as_two_resource_classes_is_a_named_error() {
    let cases = [
        (Kind::Uniform, Kind::Storage),
        (Kind::Uniform, Kind::Combined),
        (Kind::Sampler, Kind::Image),
    ];
    for (first, second) in cases {
        let vertex = stage(VERTEX, &[res(0, Kind::Uniform), res(1, first)]);
        let fragment = stage(FRAGMENT, &[res(1, second)]);
        let error = prepare_stages(&[&vertex, &fragment]).unwrap_err();
        assert!(
            matches!(
                error,
                PrepareError::BindingClass {
                    set: 0,
                    binding: 1,
                    ..
                }
            ),
            "{first:?} and {second:?} gave {error}"
        );
        assert_eq!(error.cause(), "binding class");
    }
}

#[test]
fn one_stage_using_a_binding_as_two_classes_is_a_named_error() {
    let module = stage(FRAGMENT, &[res(1, Kind::Uniform), res(1, Kind::Storage)]);
    let error = prepare_stages(&[&module]).unwrap_err();
    assert!(
        matches!(error, PrepareError::BindingClass { .. }),
        "{error}"
    );
}

#[test]
fn a_malformed_stage_is_an_error_whichever_stage_it_is() {
    let good = stage(FRAGMENT, &[res(0, Kind::Combined)]);
    let mut broken = good.clone();
    let last = broken.last_mut().unwrap();
    *last = op(56, &[0])[0];
    for stages in [[&good, &broken], [&broken, &good]] {
        let stages: Vec<&[u32]> = stages.iter().map(|words| words.as_slice()).collect();
        assert!(matches!(
            prepare_stages(&stages),
            Err(PrepareError::Module(_))
        ));
    }
    assert!(matches!(
        prepare_stages(&[&good[..4]]),
        Err(PrepareError::Module(_))
    ));
}
