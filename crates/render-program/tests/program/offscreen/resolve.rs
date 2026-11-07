//! Choosing a pass and a variant of a player shader.

use std::sync::Arc;

use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader, ResolveError};

use worldrunner_render_program::unity::synthetic::shaders::{POSITION_AND_UV, two_passes};

fn shader() -> ProgramShader {
    let synthetic = two_passes();
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

fn tag(values: &[&str]) -> PassSelector {
    PassSelector::Tag {
        key: "lightmode".to_owned(),
        values: values.iter().map(|&value| value.to_owned()).collect(),
    }
}

#[test]
fn a_pass_is_chosen_by_index_or_by_the_first_tag_value_any_pass_has() {
    let mut shader = shader();
    assert_eq!(shader.pass_count(), 2);
    assert_eq!(
        shader
            .resolve(&PassSelector::Index(1), &["K_OTHER"])
            .unwrap()
            .pass,
        1
    );
    assert_eq!(
        shader.resolve(&PassSelector::Index(0), &[]).unwrap().pass,
        0
    );
    // The first value in the caller's list that a pass has decides, not the order of the passes.
    assert_eq!(
        shader.resolve(&tag(&["Alpha", "Beta"]), &[]).unwrap().pass,
        0
    );
    assert_eq!(
        shader
            .resolve(&tag(&["Gamma", "Beta", "Alpha"]), &["K_OTHER"])
            .unwrap()
            .pass,
        1
    );
}

#[test]
fn no_pass_is_a_named_error_and_never_another_pass() {
    let mut shader = shader();
    assert!(matches!(
        shader.resolve(&PassSelector::Index(2), &[]),
        Err(ResolveError::NoPass { .. })
    ));
    assert!(matches!(
        shader.resolve(&tag(&["Gamma"]), &[]),
        Err(ResolveError::NoPass { .. })
    ));
    // The tag value is compared exactly.
    assert!(matches!(
        shader.resolve(&tag(&["alpha"]), &[]),
        Err(ResolveError::NoPass { .. })
    ));
}

#[test]
fn a_variant_has_exactly_the_keywords_the_pass_declares_that_are_enabled() {
    let mut shader = shader();
    let plain = shader.resolve(&PassSelector::Index(0), &[]).unwrap();
    let bluer = shader
        .resolve(&PassSelector::Index(0), &["K_BLUER"])
        .unwrap();
    assert_ne!(plain.blob, bluer.blob);
    assert_eq!(bluer.variant.fragment.module().entry_point, "fs_bluer");
    assert_eq!(plain.variant.fragment.module().entry_point, "fs_main");
    // Pass 0 declares only K_BLUER, so K_OTHER and a keyword the shader does not list cannot
    // change its variant.
    let same = shader
        .resolve(&PassSelector::Index(0), &["K_OTHER", "K_NOT_IN_THE_SHADER"])
        .unwrap();
    assert_eq!(same.blob, plain.blob);
}

#[test]
fn a_keyword_set_without_a_variant_is_a_named_error_and_never_a_fallback() {
    let mut shader = shader();
    // Pass 1 has no variant for the empty set, nor for K_BLUER alone.
    for keywords in [&[][..], &["K_BLUER"][..]] {
        let error = shader
            .resolve(&PassSelector::Index(1), keywords)
            .err()
            .unwrap();
        assert!(
            matches!(&error, ResolveError::NoVariant { pass: 1, .. }),
            "{keywords:?}: {error}"
        );
    }
    assert!(
        shader
            .resolve(&PassSelector::Index(1), &["K_OTHER", "K_BLUER"])
            .is_ok()
    );
}

#[test]
fn a_resolved_variant_carries_its_stage_programs_parameters_and_vertex_inputs() {
    let mut shader = shader();
    let resolved = shader.resolve(&PassSelector::Index(0), &[]).unwrap();
    let variant = &resolved.variant;
    assert_eq!(variant.vertex.module().entry_point, "vs_main");
    assert_eq!(variant.channels.channels, POSITION_AND_UV);
    assert_eq!(variant.channels.source_map, 0b1_0001);
    let names: Vec<_> = variant
        .parameters
        .constant_buffers
        .iter()
        .map(|buffer| buffer.name.as_str())
        .collect();
    assert_eq!(names, ["Material", "Draw"]);
    assert!(
        worldrunner_render_program::unity::layout::ProgramLayout::build(&[
            &variant.vertex,
            &variant.fragment
        ])
        .is_ok()
    );
}

#[test]
fn a_variant_is_prepared_once_and_shared_by_every_resolution_of_it() {
    let mut shader = shader();
    let first = shader.resolve(&PassSelector::Index(0), &[]).unwrap();
    let again = shader.resolve(&tag(&["Alpha"]), &["K_OTHER"]).unwrap();
    assert!(Arc::ptr_eq(&first.variant, &again.variant));
    assert_eq!(shader.cached_variants(), 1);
    shader
        .resolve(&PassSelector::Index(0), &["K_BLUER"])
        .unwrap();
    assert_eq!(shader.cached_variants(), 2);
}

/// The two passes with the first one's tags removed: pass 0 is untagged and pass 1 is `Beta`.
fn first_untagged() -> ProgramShader {
    let mut synthetic = two_passes();
    let first = &mut synthetic.shader.parsed.sub_shaders[0].passes[0];
    first.tags.clear();
    first.state.tags.clear();
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

fn untagged_as(values: &[&str], untagged: &str) -> PassSelector {
    PassSelector::TagWithUntagged {
        key: "lightmode".to_owned(),
        values: values.iter().map(|&value| value.to_owned()).collect(),
        untagged: untagged.to_owned(),
    }
}

#[test]
fn a_pass_without_the_tag_counts_as_the_value_given_for_untagged_passes() {
    let mut shader = first_untagged();
    assert!(matches!(
        shader.resolve(&tag(&["Alpha"]), &[]),
        Err(ResolveError::NoPass { .. })
    ));
    assert_eq!(
        shader
            .resolve(&untagged_as(&["Alpha"], "Alpha"), &[])
            .unwrap()
            .pass,
        0
    );
    // The first value in the caller's list that a pass has decides, tagged or not.
    assert_eq!(
        shader
            .resolve(&untagged_as(&["Beta", "Alpha"], "Alpha"), &["K_OTHER"])
            .unwrap()
            .pass,
        1
    );
    // A value that is neither a pass's tag nor the untagged value matches nothing.
    assert!(matches!(
        shader.resolve(&untagged_as(&["Gamma"], "Alpha"), &[]),
        Err(ResolveError::NoPass { .. })
    ));
}
