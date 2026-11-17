//! Which pass of a shader the player's cameras draw, with predicted passes from synthetic shaders.
//!
//! The rule rests on counts from the Unity 6000.3.15f1 player of Old World: 603 of its 664
//! passes have no `LightMode` tag, 147 of its 171 shaders have none that has one, every shader a
//! camera drew in a run has exactly one pass and that pass has no tag, and no pass carries the
//! `Always` or `SRPDefaultUnlit` value that the camera selects. A pass with no tag therefore
//! counts as `Always`, the camera's last choice, and any tagged pass the camera selects wins over
//! it. A pass tagged with a value the camera does not select is never drawn.
//!
//! A renderer list asks for its own tags and never for `Always`. For a list a pass with no tag
//! counts as `SRPDefaultUnlit`, so only a list that names `SRPDefaultUnlit` draws it, and a pass
//! tagged `Always` is drawn by a list only when the list names `Always`.

use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader, ResolveError};
use worldrunner_render_program::unity::synthetic::shaders::{no_cull, tint_variant};
use worldrunner_render_program::unity::synthetic::{PassSpec, shader};

use super::super::frame::Settings;
use super::super::lists::list_pass_selector;

/// A shader whose passes carry `tags` in order, `None` for a pass without a `LightMode`.
fn shader_with(tags: &[Option<&'static str>]) -> ProgramShader {
    let passes: Vec<PassSpec> = tags
        .iter()
        .map(|&tag| PassSpec {
            tags: tag.map(|value| ("LIGHTMODE", value)).into_iter().collect(),
            variants: vec![tint_variant(&[], "fs_main")],
        })
        .collect();
    let synthetic = no_cull(shader(&[], &passes));
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

/// The pass the player's cameras draw, `None` when there is none.
fn drawn(tags: &[Option<&'static str>]) -> Option<usize> {
    resolved(&Settings::cameras().pass_selector(), tags)
}

/// The pass a renderer list that asks for `asked` draws, `None` when there is none.
fn drawn_by_list(asked: &[&str], tags: &[Option<&'static str>]) -> Option<usize> {
    let asked: Vec<String> = asked.iter().map(|tag| (*tag).to_owned()).collect();
    resolved(&list_pass_selector(&asked), tags)
}

fn resolved(selector: &PassSelector, tags: &[Option<&'static str>]) -> Option<usize> {
    match shader_with(tags).resolve(selector, &[]) {
        Ok(resolved) => Some(resolved.pass),
        Err(ResolveError::NoPass { .. }) => None,
        Err(other) => panic!("{other}"),
    }
}

#[test]
fn a_shader_whose_only_pass_has_no_light_mode_is_drawn() {
    assert_eq!(drawn(&[None]), Some(0));
}

#[test]
fn a_pass_without_a_light_mode_is_drawn_beside_passes_the_camera_does_not_select() {
    assert_eq!(
        drawn(&[Some("DepthOnly"), None, Some("ShadowCaster")]),
        Some(1)
    );
    assert_eq!(drawn(&[None, Some("DepthNormals")]), Some(0));
}

#[test]
fn passes_tagged_with_modes_the_camera_does_not_select_are_left_out() {
    assert_eq!(drawn(&[Some("DepthOnly")]), None);
    assert_eq!(drawn(&[Some("ShadowCaster"), Some("DepthNormals")]), None);
    assert_eq!(
        drawn(&[Some("UniversalGBuffer"), Some("Universal2D")]),
        None
    );
    assert_eq!(drawn(&[]), None);
}

#[test]
fn a_pass_tagged_with_a_selected_mode_wins_over_one_without_a_tag() {
    assert_eq!(drawn(&[None, Some("UniversalForward")]), Some(1));
    assert_eq!(drawn(&[None, Some("ForwardBase")]), Some(1));
    assert_eq!(drawn(&[Some("UniversalForward"), None]), Some(0));
    // The camera prefers UniversalForward to UniversalForwardOnly whatever the pass order.
    assert_eq!(
        drawn(&[Some("UniversalForwardOnly"), Some("UniversalForward")]),
        Some(1)
    );
}

const URP_FORWARD: [&str; 2] = ["UniversalForward", "SRPDefaultUnlit"];

#[test]
fn a_list_naming_srp_default_unlit_draws_a_pass_with_no_light_mode() {
    assert_eq!(drawn_by_list(&URP_FORWARD, &[None]), Some(0));
    assert_eq!(
        drawn_by_list(&URP_FORWARD, &[Some("DepthOnly"), None]),
        Some(1)
    );
}

#[test]
fn a_list_that_does_not_name_srp_default_unlit_leaves_out_a_pass_with_no_light_mode() {
    assert_eq!(drawn_by_list(&["UniversalForward"], &[None]), None);
    assert_eq!(
        drawn_by_list(&["UniversalForward", "UniversalForwardOnly"], &[None]),
        None
    );
}

#[test]
fn a_list_draws_the_pass_of_the_first_tag_it_names_that_any_pass_has() {
    let passes = [None, Some("UniversalForward")];
    assert_eq!(drawn_by_list(&URP_FORWARD, &passes), Some(1));
    assert_eq!(
        drawn_by_list(&["SRPDefaultUnlit", "UniversalForward"], &passes),
        Some(0)
    );
    assert_eq!(
        drawn_by_list(&URP_FORWARD, &[Some("UniversalForward"), None]),
        Some(0)
    );
}

#[test]
fn a_pass_tagged_always_is_drawn_by_a_list_only_when_the_list_names_always() {
    assert_eq!(drawn_by_list(&URP_FORWARD, &[Some("Always")]), None);
    assert_eq!(
        drawn_by_list(&["UniversalForward", "Always"], &[Some("Always")]),
        Some(0)
    );
}

#[test]
fn a_camera_draws_a_pass_with_no_light_mode_that_a_forward_list_leaves_out() {
    assert_eq!(drawn(&[None]), Some(0));
    assert_eq!(drawn_by_list(&["UniversalForward"], &[None]), None);
}
