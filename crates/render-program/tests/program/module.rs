//! Stage modules: what naga makes of the SPIR-V and the named errors for what it cannot use.

use worldrunner_render_program::unity::module::{
    ModuleError, StageKind, StageModule, StageProgram,
};
use worldrunner_unity_serialized::shader_parameters::VariantParameters;
use worldrunner_unity_serialized::shader_program::ProgramParameters;

use crate::common::{same, spirv};
use crate::fixtures::textured;

fn module(words: Vec<u32>, entry_point: &str, kind: StageKind) -> StageModule {
    StageModule {
        words,
        entry_point: entry_point.to_owned(),
        kind,
        bindings: vec![same(0, 0)],
    }
}

fn build(module: StageModule) -> Result<StageProgram, ModuleError> {
    StageProgram::new(module, ProgramParameters::default())
}

#[test]
fn words_that_are_not_spirv_are_a_parse_error() {
    let error = build(module(vec![1, 2, 3, 4, 5, 6], "main", StageKind::Vertex)).unwrap_err();
    assert!(matches!(error, ModuleError::Parse(_)), "{error}");
}

#[test]
fn the_entry_point_must_exist_for_the_stage() {
    let words = spirv(&textured("texel"));
    let wrong_name = build(module(words.clone(), "nope", StageKind::Fragment)).unwrap_err();
    assert_eq!(
        wrong_name,
        ModuleError::EntryPointMissing {
            name: "nope".to_owned(),
            kind: StageKind::Fragment
        }
    );
    let wrong_stage = build(module(words.clone(), "vs_main", StageKind::Fragment)).unwrap_err();
    assert!(matches!(wrong_stage, ModuleError::EntryPointMissing { .. }));
    assert!(build(module(words, "vs_main", StageKind::Vertex)).is_ok());
}

#[test]
fn a_storage_texture_is_a_named_error() {
    let words = spirv(
        "@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
         @compute @workgroup_size(1) fn main() {
             textureStore(image, vec2<i32>(0, 0), vec4<f32>(1.0));
         }",
    );
    let error = build(module(words, "main", StageKind::Compute)).unwrap_err();
    assert!(
        matches!(&error, ModuleError::UnsupportedResource { what, .. } if what == "storage texture"),
        "{error}"
    );
}

#[test]
fn the_content_key_follows_the_words_entry_point_and_stage() {
    let words = spirv(&textured("texel"));
    let key = |words: &[u32], entry: &str, kind| {
        build(module(words.to_vec(), entry, kind))
            .unwrap()
            .content_key()
    };
    let base = key(&words, "fs_main", StageKind::Fragment);
    assert_eq!(base, key(&words, "fs_main", StageKind::Fragment));
    let other = spirv(&textured("texel * material.tint"));
    assert_ne!(base, key(&other, "fs_main", StageKind::Fragment));
    assert_ne!(base, key(&words, "vs_main", StageKind::Vertex));
}

#[test]
fn either_form_of_parameters_builds_the_same_module() {
    let words = spirv(&textured("texel"));
    let editor = StageProgram::new(
        module(words.clone(), "fs_main", StageKind::Fragment),
        ProgramParameters::default(),
    )
    .unwrap();
    let player = StageProgram::new(
        module(words, "fs_main", StageKind::Fragment),
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: Vec::new(),
        },
    )
    .unwrap();
    assert_eq!(editor.content_key(), player.content_key());
    assert_eq!(editor.resources(), player.resources());
}
