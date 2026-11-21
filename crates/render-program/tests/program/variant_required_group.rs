//! A pass whose variants all take one keyword of an exclusive group is chosen with the group's
//! first keyword when the draw enables none of them.
//!
//! Old World's terrain shader keeps 256 Vulkan variants on each pass. Every variant has exactly
//! one of two main-light shadow keywords, so no variant has neither, and the variant list ends
//! with the one that has the group's first keyword and nothing else. A draw that enables only
//! other keywords of the pass has no exact variant, and the player takes the group's first
//! keyword for it.

use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader, ResolveError};
use worldrunner_render_program::unity::synthetic::shaders::tint_variant;
use worldrunner_render_program::unity::synthetic::{PassSpec, VariantSpec, shader};

const STEREO: &str = "STEREO_INSTANCING_ON";
const SHADOWS: &str = "MAIN_SHADOWS";
const CASCADE: &str = "MAIN_SHADOWS_CASCADE";
const TEAM: &str = "TEAM_COLOR";
const SOFT: &str = "SOFT_SHADOWS";
const KEYWORDS: [&str; 5] = [STEREO, SHADOWS, CASCADE, TEAM, SOFT];

/// Every combination of one shadow keyword, the team colour and soft shadows, in the order of
/// the player's list: the last keyword of the group first, the all-default variant last.
fn terrain_like_variants() -> Vec<VariantSpec> {
    let mut variants = Vec::new();
    for group in [CASCADE, SHADOWS] {
        for team in [true, false] {
            for soft in [true, false] {
                let mut set = vec![group];
                set.extend(team.then_some(TEAM));
                set.extend(soft.then_some(SOFT));
                variants.push(tint_variant(&set, "fs_main"));
            }
        }
    }
    variants
}

fn program_shader(variants: Vec<VariantSpec>) -> ProgramShader {
    let mut synthetic = shader(
        &KEYWORDS,
        &[PassSpec {
            tags: Vec::new(),
            variants,
        }],
    );
    // The state mask names the stereo keyword and the team colour, as the real pass's does.
    synthetic.shader.parsed.sub_shaders[0].passes[0]
        .programs
        .vertex
        .keyword_state_mask = vec![0, 3];
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

fn blob(shader: &mut ProgramShader, keywords: &[&str]) -> Result<u32, ResolveError> {
    shader
        .resolve(&PassSelector::Index(0), keywords)
        .map(|resolved| resolved.blob)
}

#[test]
fn a_draw_without_a_shadow_keyword_takes_the_groups_first_keyword() {
    let mut shader = program_shader(terrain_like_variants());
    let first = blob(&mut shader, &[SHADOWS, TEAM]).unwrap();
    let second = blob(&mut shader, &[CASCADE, TEAM]).unwrap();
    assert_ne!(first, second);
    assert_eq!(blob(&mut shader, &[TEAM]).unwrap(), first);
    assert_eq!(
        blob(&mut shader, &[TEAM, "NOT_IN_THE_SHADER"]).unwrap(),
        first
    );
    assert_eq!(
        blob(&mut shader, &[SOFT, TEAM]).unwrap(),
        blob(&mut shader, &[SHADOWS, SOFT, TEAM]).unwrap()
    );
}

#[test]
fn nothing_enabled_takes_the_default_of_the_group_and_no_other_keyword() {
    let mut shader = program_shader(terrain_like_variants());
    let plain = blob(&mut shader, &[SHADOWS]).unwrap();
    assert_eq!(blob(&mut shader, &[]).unwrap(), plain);
    assert_ne!(blob(&mut shader, &[CASCADE]).unwrap(), plain);
}

#[test]
fn an_enabled_member_of_the_group_is_never_replaced_by_the_default() {
    let mut shader = program_shader(terrain_like_variants());
    let cascade = blob(&mut shader, &[CASCADE, SOFT]).unwrap();
    assert_ne!(cascade, blob(&mut shader, &[SHADOWS, SOFT]).unwrap());
    assert_ne!(cascade, blob(&mut shader, &[SOFT]).unwrap());
}

#[test]
fn two_members_of_one_group_have_no_variant() {
    let mut shader = program_shader(terrain_like_variants());
    let error = blob(&mut shader, &[SHADOWS, CASCADE]).unwrap_err();
    assert!(
        matches!(error, ResolveError::NoVariant { pass: 0, .. }),
        "{error}"
    );
}

#[test]
fn a_keyword_every_variant_carries_is_not_a_choice_and_gets_no_default() {
    let mut shader = program_shader(vec![
        tint_variant(&[TEAM], "fs_main"),
        tint_variant(&[TEAM, SOFT], "fs_main"),
    ]);
    let error = blob(&mut shader, &[]).unwrap_err();
    assert!(
        matches!(error, ResolveError::NoVariant { pass: 0, .. }),
        "{error}"
    );
    assert!(blob(&mut shader, &[TEAM]).is_ok());
}

#[test]
fn an_optional_group_gets_no_default_when_a_variant_has_none_of_it() {
    let mut shader = program_shader(vec![
        tint_variant(&[], "fs_main"),
        tint_variant(&[SHADOWS], "fs_main"),
        tint_variant(&[CASCADE], "fs_main"),
    ]);
    let none = blob(&mut shader, &[]).unwrap();
    assert_ne!(none, blob(&mut shader, &[SHADOWS]).unwrap());
    assert!(blob(&mut shader, &[SHADOWS, CASCADE]).is_err());
}
