//! Which pass of a shader the player's cameras draw, with predicted passes from synthetic shaders.
//!
//! The rule rests on counts from the Unity 6000.3.15f1 player of Old World: 603 of its 664
//! passes have no `LightMode` tag, 147 of its 171 shaders have none that has one, every shader a
//! camera drew in a run has exactly one pass and that pass has no tag, and no pass carries the
//! `Always` or `SRPDefaultUnlit` value that the camera selects. A pass with no tag therefore
//! counts as `Always`, the camera's last choice, and any tagged pass the camera selects wins over
//! it. A pass tagged with a value the camera does not select is never drawn.

use worldrunner_render_program::shader::{ProgramShader, ResolveError};
use worldrunner_render_program::synthetic::shaders::{no_cull, tint_variant};
use worldrunner_render_program::synthetic::{PassSpec, shader};

use super::super::frame::Settings;

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
    let selector = Settings::cameras().pass_selector();
    match shader_with(tags).resolve(&selector, &[]) {
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
