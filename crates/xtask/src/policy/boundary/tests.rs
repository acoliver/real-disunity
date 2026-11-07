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
            kind: None,
            features: Vec::new(),
            uses_default_features: true,
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

/// Workspace metadata in `cargo metadata`'s shape: the render-program package with its feature
/// table, and one consumer package with the given dependency entries.
fn workspace(consumer: &str, dependencies: serde_json::Value) -> Metadata {
    let package = |name: &str, dependencies: serde_json::Value, features: serde_json::Value| {
        serde_json::json!({
            "id": name,
            "name": name,
            "manifest_path": format!("/workspace/{name}/Cargo.toml"),
            "dependencies": dependencies,
            "targets": [],
            "features": features,
        })
    };
    serde_json::from_value(serde_json::json!({
        "packages": [
            package(
                boundary().package,
                serde_json::json!([]),
                serde_json::json!({
                    "unity": ["dep:worldrunner-unity-serialized"],
                    "everything": ["unity"],
                    "default": [],
                }),
            ),
            package(consumer, dependencies, serde_json::json!({})),
        ],
        "workspace_members": [boundary().package, consumer],
        "workspace_root": "/workspace",
        "resolve": { "nodes": [] },
    }))
    .unwrap()
}

/// One entry of a package's `dependencies` on render-program, as `cargo metadata` prints it.
fn on_render_program(extra: serde_json::Value) -> serde_json::Value {
    let mut entry = serde_json::json!({
        "name": boundary().package,
        "path": "/workspace/render-program",
        "kind": null,
        "rename": null,
        "features": [],
        "uses_default_features": true,
    });
    let serde_json::Value::Object(overrides) = extra else {
        panic!("a dependency override is an object");
    };
    for (key, value) in overrides {
        entry[key] = value;
    }
    entry
}

fn manifest_errors(consumer: &str, extra: serde_json::Value) -> Vec<String> {
    let data = workspace(consumer, serde_json::json!([on_render_program(extra)]));
    let mut errors = Vec::new();
    feature_errors(boundary(), &data, &data.packages[0], &mut errors);
    errors
}

#[test]
fn a_normal_dependency_with_the_adapter_feature_is_rejected_for_other_crates() {
    let errors = manifest_errors(
        "worldrunner-render-scene",
        serde_json::json!({ "features": ["unity"] }),
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("worldrunner-render-scene"), "{errors:?}");
    assert!(
        errors[0].contains("worldrunner-render-program"),
        "{errors:?}"
    );
    assert!(errors[0].contains("`unity` feature"), "{errors:?}");
    assert!(
        errors[0].contains("worldrunner-play, worldrunner-unity-import"),
        "{errors:?}"
    );
}

#[test]
fn a_dependency_without_the_adapter_feature_is_allowed_everywhere() {
    for extra in [
        serde_json::json!({}),
        serde_json::json!({ "features": ["other"] }),
        serde_json::json!({ "kind": "build", "uses_default_features": false }),
    ] {
        let errors = manifest_errors("worldrunner-render-scene", extra);
        assert!(errors.is_empty(), "{errors:?}");
    }
}

#[test]
fn a_dev_dependency_with_the_adapter_feature_is_allowed() {
    let errors = manifest_errors(
        "worldrunner-render-scene",
        serde_json::json!({ "kind": "dev", "features": ["unity"] }),
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn the_play_and_unity_import_crates_may_enable_the_adapter_feature() {
    for consumer in ["worldrunner-play", "worldrunner-unity-import"] {
        for kind in [serde_json::Value::Null, "build".into(), "dev".into()] {
            let errors = manifest_errors(
                consumer,
                serde_json::json!({ "kind": kind, "features": ["unity"] }),
            );
            assert!(errors.is_empty(), "{consumer}: {errors:?}");
        }
    }
}

#[test]
fn a_build_dependency_with_the_adapter_feature_is_rejected() {
    let errors = manifest_errors(
        "worldrunner-render-scene",
        serde_json::json!({ "kind": "build", "features": ["unity"] }),
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("build dependency"), "{errors:?}");
    assert!(errors[0].contains("worldrunner-render-scene"), "{errors:?}");
}

#[test]
fn a_renamed_dependency_with_the_adapter_feature_is_rejected() {
    let errors = manifest_errors(
        "worldrunner-render-scene",
        serde_json::json!({ "rename": "programs", "features": ["unity"] }),
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("worldrunner-render-scene"), "{errors:?}");
}

#[test]
fn a_target_specific_dependency_counts_as_a_normal_one() {
    let errors = manifest_errors(
        "worldrunner-render-scene",
        serde_json::json!({ "target": "cfg(unix)", "features": ["unity"] }),
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_feature_that_enables_the_adapter_feature_counts_as_enabling_it() {
    let errors = manifest_errors(
        "worldrunner-render-scene",
        serde_json::json!({ "features": ["everything"] }),
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn default_features_count_when_the_package_makes_the_adapter_a_default() {
    let mut data = workspace(
        "worldrunner-render-scene",
        serde_json::json!([on_render_program(serde_json::json!({}))]),
    );
    let mut errors = Vec::new();
    feature_errors(boundary(), &data, &data.packages[0], &mut errors);
    assert!(errors.is_empty(), "{errors:?}");

    data.packages[0]
        .features
        .insert("default".into(), vec!["unity".into()]);
    feature_errors(boundary(), &data, &data.packages[0], &mut errors);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("default features"), "{errors:?}");

    data.packages[1].dependencies[0].uses_default_features = false;
    let mut errors = Vec::new();
    feature_errors(boundary(), &data, &data.packages[0], &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
}
