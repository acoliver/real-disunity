//! A pass's variants are chosen by the keywords they differ by, whatever its state mask lists.
//!
//! The Unity 6 player stores every Vulkan variant of a pass on the vertex program. That program's
//! keyword state mask names the stereo keywords, which no variant uses, while the variants differ
//! by fragment keywords such as a bloom or grading option.

use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader};
use worldrunner_render_program::unity::synthetic::shaders::tint_variant;
use worldrunner_render_program::unity::synthetic::{PassSpec, shader};

const KEYWORDS: [&str; 3] = ["STEREO_INSTANCING_ON", "K_BLOOM", "K_GRADING"];

/// A pass whose variants are the plain one, `K_BLOOM` and `K_BLOOM` with `K_GRADING`. `mask`
/// names the keywords of the vertex program's state mask.
fn pass_with_mask(mask: &[&str]) -> ProgramShader {
    let mut synthetic = shader(
        &KEYWORDS,
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![
                tint_variant(&[], "fs_main"),
                tint_variant(&["K_BLOOM"], "fs_main"),
                tint_variant(&["K_BLOOM", "K_GRADING"], "fs_main"),
            ],
        }],
    );
    let indices = mask
        .iter()
        .map(|name| {
            let at = KEYWORDS.iter().position(|keyword| keyword == name).unwrap();
            u16::try_from(at).unwrap()
        })
        .collect();
    synthetic.shader.parsed.sub_shaders[0].passes[0]
        .programs
        .vertex
        .keyword_state_mask = indices;
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

/// The record of the variant `keywords` select.
fn blob(shader: &mut ProgramShader, keywords: &[&str]) -> u32 {
    shader
        .resolve(&PassSelector::Index(0), keywords)
        .map(|resolved| resolved.blob)
        .unwrap()
}

#[test]
fn a_state_mask_of_stereo_keywords_does_not_hide_the_keywords_the_variants_differ_by() {
    let mut unmasked = pass_with_mask(&[]);
    let mut masked = pass_with_mask(&["STEREO_INSTANCING_ON"]);
    let none = blob(&mut unmasked, &[]);
    let bloom = blob(&mut unmasked, &["K_BLOOM"]);
    let both = blob(&mut unmasked, &["K_BLOOM", "K_GRADING"]);
    assert_ne!(none, bloom);
    assert_ne!(bloom, both);
    assert_eq!(blob(&mut masked, &[]), none);
    assert_eq!(blob(&mut masked, &["K_BLOOM"]), bloom);
    assert_eq!(blob(&mut masked, &["K_BLOOM", "K_GRADING"]), both);
}

#[test]
fn an_enabled_keyword_in_the_state_mask_that_no_variant_uses_changes_nothing() {
    let mut masked = pass_with_mask(&["STEREO_INSTANCING_ON"]);
    let bloom = blob(&mut masked, &["K_BLOOM"]);
    assert_eq!(
        blob(&mut masked, &["STEREO_INSTANCING_ON", "K_BLOOM"]),
        bloom
    );
}
