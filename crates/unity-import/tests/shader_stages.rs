//! The stage modules of a variant, prepared together, still pass naga. The modules are the
//! ones `unity-serialized` checks the numbering of.

#[path = "../../unity-serialized/tests/support/stage_modules.rs"]
mod stage_modules;

use worldrunner_unity_import::shader_prep::{check_with_naga, prepare, prepare_stages};

#[test]
fn naga_rejects_the_unprepared_stage_that_splits_a_sampler() {
    let [_, fragment] = <[_; 2]>::try_from(stage_modules::fragment_only_split()).unwrap();
    let error = check_with_naga(&fragment).err().expect("naga rejects it");
    assert_eq!(error.cause(), "parse InvalidId");
}

#[test]
fn naga_accepts_every_stage_of_every_variant_prepared_together() {
    for (n, variant) in stage_modules::variants().iter().enumerate() {
        let stages: Vec<&[u32]> = variant.iter().map(Vec::as_slice).collect();
        let prepared = prepare_stages(&stages).unwrap();
        assert_eq!(prepared.modules.len(), stages.len());
        for module in &prepared.modules {
            if let Err(error) = check_with_naga(&module.words) {
                panic!("variant {n}: {error}");
            }
            assert_eq!(module.bindings, prepared.modules[0].bindings);
        }
    }
}

#[test]
fn naga_accepts_each_stage_prepared_alone_too() {
    for variant in stage_modules::variants() {
        for stage in &variant {
            let prepared = prepare(stage).unwrap();
            assert!(check_with_naga(&prepared.words).is_ok());
        }
    }
}
