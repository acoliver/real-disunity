//! Paired consumers must capture quad inputs before rejecting material work.
use super::*;
use crate::scene_metadata::receiver::{ReceiverLayout, consumer_source, function_source};

#[test]
fn unpaired_card_source_matches_the_frozen_base_evaluator() {
    let baseline = include_str!("card_rejection_reference.wgsl").trim_end();
    assert_eq!(
        function_source(crate::shader::ENVIRONMENT_WGSL, "scene_card_shade"),
        baseline
    );
    let shadowed = crate::shadows::receiver_sources(true, true, true).0;
    assert_eq!(function_source(&shadowed, "scene_card_shade"), baseline);
}

fn paired_sources() -> Vec<String> {
    [ReceiverLayout::Front, ReceiverLayout::Opposing]
        .into_iter()
        .flat_map(|layout| {
            [false, true].into_iter().flat_map(move |reflection| {
                [false, true].into_iter().map(move |velocity| {
                    let source = crate::shadows::receiver_sources(true, reflection, velocity).0;
                    consumer_source(&source, layout, false)
                })
            })
        })
        .collect()
}

#[test]
fn paired_cards_reject_before_surface_normal_contact_decals_and_receivers() {
    for source in paired_sources() {
        assert!(check(&source, "scene_card_shade"));
        let body = function_source(&source, "scene_card_shade");
        let rejection = body
            .find("if !visible || !scene_geometry_covered(input)")
            .unwrap();
        let alpha = body.find("var albedo_sample = scene_albedo(").unwrap();
        assert!(alpha < rejection);
        for name in [
            "scene_surface_channels_prepared(",
            "sample_normal_prepared(",
            "scene_contact(",
            "scene_project_decals_prepared(",
            "scene_prepare_card_shadow(",
            "scene_receiver_back_card(",
        ] {
            assert!(body.find(name).unwrap() > rejection, "{name}");
        }
        for name in [
            "scene_atlas_footprint(albedo_texture",
            "scene_prepare_surface_channels(",
            "scene_prepare_card_occlusion(",
            "scene_prepare_card_emission(",
            "scene_prepare_normal(",
            "scene_prepare_decals(",
        ] {
            assert!(body.find(name).unwrap() < alpha, "{name}");
        }
    }
}

#[test]
fn paired_guard_detects_each_preparation_moved_after_rejection() {
    for source in paired_sources() {
        let body = function_source(&source, "scene_card_shade");
        let rejection = body.find("if !visible ||").unwrap();
        for line in body[..rejection].lines().filter(|line| {
            line.contains(" = scene_prepare_") || line.contains(" = scene_atlas_footprint(")
        }) {
            let (base, late) = if line.contains("albedo_footprint") {
                (
                    body.to_owned(),
                    line.replace("albedo_footprint", "mutation_footprint"),
                )
            } else {
                (body.replace(line, ""), line.to_owned())
            };
            let changed = base.replace("    if masked {", &format!("{late}\n    if masked {{"));
            assert_ne!(changed, body);
            assert!(
                !check(&source.replace(body, &changed), "scene_card_shade"),
                "{line}"
            );
        }
    }
}

#[test]
fn masked_caster_rejects_geometry_before_unchanged_alpha_sum() {
    let source = crate::shadows::depth_source();
    assert!(check(&source, "fs_depth_masked"));
    let body = function_source(&source, "fs_depth_masked");
    let footprint = body.find("let footprint = scene_atlas_footprint(").unwrap();
    let rejection = body.find("if !scene_geometry_covered(input)").unwrap();
    let sample = body
        .find("scene_atlas_sample(albedo_texture, input, footprint).a")
        .unwrap();
    assert!(footprint < rejection && rejection < sample);
    let line = "let footprint = scene_atlas_footprint(albedo_texture, input);";
    let changed = body
        .replace(line, "")
        .replace("let alpha =", &format!("{line}\nlet alpha ="));
    assert!(!check(&source.replace(body, &changed), "fs_depth_masked"));
}

#[test]
fn paired_normal_preparation_keeps_cotangent_arithmetic_at_the_sample() {
    for source in paired_sources() {
        let prepare = function_source(&source, "scene_prepare_normal");
        for operand in [
            "dpdx(position)",
            "dpdy(position)",
            "dpdx(input.texcoord)",
            "dpdy(input.texcoord)",
        ] {
            assert!(prepare.contains(operand), "{operand}");
        }
        assert!(!prepare.contains("cotangent_frame("));
        let old = function_source(&source, "cotangent_frame");
        let new = function_source(&source, "scene_card_cotangent_frame");
        let arithmetic = "    let perpendicular_dy =";
        assert_eq!(
            &old[old.find(arithmetic).unwrap()..],
            &new[new.find(arithmetic).unwrap()..]
        );
        assert!(
            function_source(&source, "sample_normal_prepared")
                .contains("scene_card_cotangent_frame(geometric, prepared)")
        );
    }
}
