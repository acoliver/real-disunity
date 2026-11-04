use super::*;
use crate::scratch::Directory;

fn boundary() -> &'static Boundary {
    &BOUNDARIES[0]
}

/// The crossings found in `source`, written as the file at `path` under the package's `src`.
fn found(path: &str, source: &str) -> Vec<String> {
    let file = syn::parse_file(source).unwrap();
    violations(boundary(), &format!("src/{path}"), path == "lib.rs", &file)
}

fn rejects(path: &str, source: &str, needle: &str) {
    let errors = found(path, source);
    assert!(
        errors.iter().any(|error| error.contains(needle)),
        "{source}\n{errors:?}"
    );
}

#[test]
fn neutral_code_that_never_names_the_adapter_or_its_crate_is_clean() {
    let source = "use crate::module::StageProgram;\n\
                  /// Mentions worldrunner_unity_serialized and crate::unity in prose.\n\
                  pub fn f() -> &'static str { \"worldrunner_unity_serialized\" }";
    assert!(found("layout.rs", source).is_empty());
}

#[test]
fn a_gated_adapter_declaration_in_the_crate_root_is_allowed() {
    let source = "#[cfg(feature = \"unity\")]\npub mod unity;\npub mod layout;";
    assert!(found("lib.rs", source).is_empty());
}

#[test]
fn a_plain_import_of_the_format_crate_is_rejected() {
    rejects(
        "layout.rs",
        "use worldrunner_unity_serialized::shader_program::Program;",
        "names the worldrunner-unity-serialized crate",
    );
}

#[test]
fn renamed_imports_are_rejected_however_they_are_written() {
    for source in [
        "use worldrunner_unity_serialized as ser;",
        "use worldrunner_unity_serialized::shader_program::Program as Neutral;",
        "use worldrunner_unity_serialized::{shader_program::{Program as P, SubProgram}};",
        "use ::worldrunner_unity_serialized::shader_state::*;",
        "pub use worldrunner_unity_serialized::shader_program as programs;",
        "extern crate worldrunner_unity_serialized as ser;",
        "fn f() { use worldrunner_unity_serialized::shader_program::Program as P; }",
    ] {
        rejects("layout.rs", source, "worldrunner-unity-serialized crate");
    }
}

#[test]
fn the_format_crate_in_a_signature_a_field_or_a_bound_is_rejected() {
    for source in [
        "pub fn f() -> ::worldrunner_unity_serialized::shader_program::Program { todo!() }",
        "pub struct S { pub program: worldrunner_unity_serialized::shader_program::Program }",
        "pub fn g<T: worldrunner_unity_serialized::Marker>(_: T) {}",
        "pub type Alias = worldrunner_unity_serialized::shader_state::ShaderState;",
    ] {
        rejects("program.rs", source, "worldrunner-unity-serialized crate");
    }
}

#[test]
fn the_format_crate_in_macro_input_or_an_attribute_is_rejected() {
    rejects(
        "program.rs",
        "fn f() { let _ = wrap!(worldrunner_unity_serialized::shader_program::Program::default()); }",
        "worldrunner-unity-serialized crate",
    );
    rejects(
        "program.rs",
        "#[cfg_attr(test, derive(worldrunner_unity_serialized::Marker))] struct S;",
        "worldrunner-unity-serialized crate",
    );
}

#[test]
fn a_root_module_may_not_reexport_or_alias_the_adapter() {
    for source in [
        "pub use crate::unity::runtime::ProgramRuntime;",
        "pub use crate::unity::*;",
        "pub use self::unity::module::StageProgram as Stage;",
        "pub use super::unity::state::RenderState;",
        "pub type Stage = crate::unity::module::StageProgram;",
        "pub fn f(_: crate::unity::state::RenderState) {}",
        "use crate::{unity::variant::select_variant, module::StageProgram};",
    ] {
        rejects("runtime.rs", source, "adapter module");
    }
    rejects("lib.rs", "pub use unity::runtime::*;", "adapter module");
    rejects(
        "lib.rs",
        "pub use unity::module as stage;",
        "adapter module",
    );
}

#[test]
fn the_adapter_declaration_needs_the_crate_root_and_its_feature() {
    rejects(
        "lib.rs",
        "pub mod unity;",
        "without #[cfg(feature = \"unity\")]",
    );
    rejects(
        "lib.rs",
        "#[cfg(feature = \"other\")]\npub mod unity;",
        "without #[cfg(feature = \"unity\")]",
    );
    rejects(
        "module.rs",
        "#[cfg(feature = \"unity\")]\npub mod unity;",
        "outside the crate root",
    );
}

#[test]
fn a_module_cannot_be_pointed_at_the_adapter_files() {
    rejects(
        "runtime.rs",
        "#[path = \"unity/runtime.rs\"]\npub mod runtime;",
        "points a module at the adapter's files",
    );
}

#[test]
fn each_crossing_is_reported_with_its_file_and_line() {
    let errors = found("layout.rs", "\n\nuse worldrunner_unity_serialized::X;");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("src/layout.rs:3:"), "{errors:?}");
}

#[test]
fn a_renamed_dependency_on_the_format_package_is_rejected() {
    let renamed = |rename: Option<&str>| {
        let dependency = Dependency {
            name: boundary().foreign_package.to_owned(),
            path: None,
            rename: rename.map(str::to_owned),
        };
        let mut errors = Vec::new();
        dependency_errors(boundary(), &[dependency], &mut errors);
        errors
    };
    assert!(renamed(None).is_empty());
    assert!(renamed(Some("serialized"))[0].contains("is renamed"));
}

#[test]
fn only_files_under_the_adapter_directory_may_cross() {
    let dir = Directory::new("boundary-test").unwrap();
    let crossing =
        "use worldrunner_unity_serialized::shader_program::Program;\npub fn f(_: Program) {}\n";
    dir.write(
        "src/lib.rs",
        "#[cfg(feature = \"unity\")]\npub mod unity;\n",
    )
    .unwrap();
    dir.write("src/unity/mod.rs", crossing).unwrap();
    dir.write("src/unity/deep/inner.rs", crossing).unwrap();
    let root = metadata::canonical(dir.path()).unwrap();
    let src = metadata::canonical(&dir.path().join("src")).unwrap();
    let mut errors = Vec::new();
    source_errors(boundary(), &root, &src, &mut errors).unwrap();
    assert!(errors.is_empty(), "{errors:?}");

    dir.write("src/layout.rs", crossing).unwrap();
    dir.write("src/nested/unity.rs", crossing).unwrap();
    source_errors(boundary(), &root, &src, &mut errors).unwrap();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].starts_with("src/layout.rs:1:"), "{errors:?}");
    assert!(
        errors[1].starts_with("src/nested/unity.rs:1:"),
        "{errors:?}"
    );
}
