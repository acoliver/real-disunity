use super::*;
use crate::scratch::Directory;

const FILE: &str = "crates/model/src/lib.rs";
const VALID: &str = "#[serde(deny_unknown_fields)] pub struct Settings { pub gain: f32 }";
const CONSUMER: &str = "#[test] fn reaches_consumer_gain() { let input = Settings { gain: 0.5 }; assert_eq!(consume(input.gain), 0.5); }";

fn fixture(source: &str) -> Directory {
    let dir = Directory::new("policy-test").unwrap();
    dir.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/model\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    dir.write(
        "crates/model/Cargo.toml",
        "[package]\nname = \"worldrunner-world-model\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    dir.write(FILE, source).unwrap();
    dir.write("crates/model/tests/consumer.rs", CONSUMER)
        .unwrap();
    dir.write(config::FILE, &config_text("dto", true)).unwrap();
    dir
}

fn config_text(role: &str, coverage: bool) -> String {
    let mut text = format!(
        "literal_scopes = [{FILE:?}]\nowners = []\n[[types]]\nname = \"Settings\"\nfile = {FILE:?}\nrole = {role:?}\n"
    );
    if coverage {
        text.push_str("[[coverage]]\ndto = \"Settings\"\nfield = \"gain\"\ntest = \"crates/model/tests/consumer.rs::reaches_consumer_gain\"\n");
    } else {
        text = format!("coverage = []\n{text}");
    }
    text
}

fn check_fixture(dir: &Directory) -> Result<(), String> {
    let data = metadata::read(dir.path(), &["--offline", "--all-features"])?;
    check_sources(dir.path(), &data)
}

#[test]
fn clean_fixture_passes_without_history_or_exception_data() {
    check_fixture(&fixture(VALID)).unwrap();
}

#[test]
fn metadata_source_paths_are_normalized_without_skipping_cpu_checks() {
    let dir = fixture(VALID);
    let mut data = metadata::read(dir.path(), &["--offline", "--all-features"]).unwrap();
    for package in &mut data.packages {
        package.manifest_path = dir.path().join("crates/model/../model/Cargo.toml");
    }
    check_sources(dir.path(), &data).unwrap();
    dir.write(
        FILE,
        &format!("{VALID}\nfn f() {{ let _ = std::env::var(\"X\"); }}"),
    )
    .unwrap();
    let error = check_sources(dir.path(), &data).unwrap_err();
    assert!(error.contains("environment"), "{error}");
}

#[test]
fn strict_dto_shape_rejects_unknown_fields_defaults_and_options() {
    for (source, expected) in [
        (
            "pub struct Settings { pub gain: f32 }",
            "requires serde deny_unknown_fields",
        ),
        (
            "#[serde(deny_unknown_fields, default)] pub struct Settings { pub gain: f32 }",
            "cannot have serde default",
        ),
        (
            "#[serde(deny_unknown_fields)] pub struct Settings { #[serde(default)] pub gain: f32 }",
            "cannot default",
        ),
        (
            "#[serde(deny_unknown_fields)] pub struct Settings { pub gain: Vec<Option<f32>> }",
            "cannot use Option",
        ),
        (
            "#[serde(deny_unknown_fields)] pub enum Settings { Value { #[cfg_attr(feature = \"x\", serde(default))] gain: f32 } }",
            "cannot default",
        ),
    ] {
        let error = check_fixture(&fixture(source)).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn default_cannot_hide_behind_derive_impl_inherent_function_or_import_alias() {
    for suffix in [
        "impl Default for Settings { fn default() -> Self { panic!() } }",
        "impl Settings { pub fn default() -> Self { panic!() } }",
        "use std::default::Default as Fallback; impl Fallback for Settings { fn default() -> Self { panic!() } }",
    ] {
        let error = check_fixture(&fixture(&format!("{VALID}\n{suffix}"))).unwrap_err();
        assert!(error.contains("cannot implement Default"), "{error}");
    }
    for derive in [
        "#[derive(Default)]",
        "#[cfg_attr(feature = \"x\", derive(Default))]",
        "use std::default::Default as Fallback; #[derive(Fallback)]",
    ] {
        let error = check_fixture(&fixture(&format!("{derive}\n{VALID}"))).unwrap_err();
        assert!(error.contains("cannot derive Default"), "{error}");
    }
}

#[test]
fn runtime_no_default_applies_to_impls_in_other_source_files() {
    let dir = fixture("pub struct Settings;");
    dir.write(config::FILE, &config_text("runtime", false))
        .unwrap();
    dir.write(
        "crates/model/src/defaults.rs",
        "impl crate::Settings { pub fn default() -> Self { Self } }",
    )
    .unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("cannot implement Default")
    );
}

#[test]
fn aliases_of_registered_types_and_option_are_rejected() {
    for suffix in [
        "type Hidden = Settings;",
        "use crate::Settings as Hidden;",
        "type Maybe = Option<f32>;",
        "use std::option::Option as Maybe;",
    ] {
        let error = check_fixture(&fixture(&format!("{VALID}\n{suffix}"))).unwrap_err();
        assert!(
            error.contains("forbidden") || error.contains("cannot shadow Option"),
            "{error}"
        );
    }
}

#[test]
fn literals_require_authority_and_unused_owners_are_errors() {
    let dir = fixture(&format!("{VALID}\nconst MAX_GAIN: f32 = 7.0;"));
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("unowned policy numeric literal")
    );
    let text = config_text("dto", true).replace("owners = []", &format!("owners = [{{ file = {FILE:?}, symbol = \"MAX_GAIN\", reason = \"Wire format ceiling\" }}]"));
    dir.write(config::FILE, &text).unwrap();
    check_fixture(&dir).unwrap();
    dir.write(FILE, VALID).unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("stale literal owner")
    );
}

#[test]
fn numeric_macro_tokens_and_negative_nonidentity_literals_are_checked() {
    for suffix in [
        "fn f() { policy!(7.0); }",
        "fn f() { policy!(-2.0); }",
        "fn f() -> f32 { -2.0 }",
        "fn f() -> f32 { 0x10 as f32 }",
    ] {
        assert!(
            check_fixture(&fixture(&format!("{VALID}\n{suffix}")))
                .unwrap_err()
                .contains("unowned policy numeric literal")
        );
    }
    let dir = fixture(&format!(
        "{VALID}\nfn f() -> [f32; 16] {{ [0.5; 16] }}\n#[cfg(test)] mod tests {{ const VALUE: f32 = 17.0; }}"
    ));
    check_fixture(&dir).unwrap();
}

#[test]
fn consumer_coverage_rejects_missing_stale_duplicate_and_fake_tests() {
    let dir = fixture(VALID);
    dir.write(config::FILE, &config_text("dto", false)).unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("missing consumer coverage")
    );
    let text = config_text("dto", true);
    dir.write(
        config::FILE,
        &text.replace("field = \"gain\"", "field = \"removed\""),
    )
    .unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("stale consumer coverage")
    );
    let coverage = text.split("[[coverage]]").nth(1).unwrap();
    dir.write(config::FILE, &format!("{text}\n[[coverage]]{coverage}"))
        .unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("duplicate consumer coverage")
    );
    dir.write(config::FILE, &text).unwrap();
    for source in [
        "fn reaches_consumer_gain() { let _ = p.gain; }",
        "#[test] fn reaches_consumer_gain() { assert!(true, \"gain\"); /* p.gain */ }",
        "#[test] #[ignore] fn reaches_consumer_gain() { assert_eq!(p.gain, 1.0); }",
    ] {
        dir.write("crates/model/tests/consumer.rs", source).unwrap();
        assert!(check_fixture(&dir).is_err(), "accepted fake test {source}");
    }
}

#[test]
fn incomplete_or_malformed_scans_fail_closed() {
    let dir = fixture(VALID);
    dir.write("crates/model/src/broken.rs", "fn broken(")
        .unwrap();
    assert!(check_fixture(&dir).unwrap_err().contains("cannot parse"));
    dir.write("crates/model/src/broken.rs", "").unwrap();
    dir.write(
        config::FILE,
        &config_text("dto", true).replace(FILE, "../outside.rs"),
    )
    .unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("without traversal")
    );
    dir.write(
        config::FILE,
        &format!("unknown = true\n{}", config_text("dto", true)),
    )
    .unwrap();
    assert!(check_fixture(&dir).unwrap_err().contains("unknown field"));
    dir.write(FILE, "pub struct Unregistered;").unwrap();
    dir.write(config::FILE, &config_text("dto", true)).unwrap();
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("exactly one definition")
    );
}

#[test]
fn environment_lookups_cannot_hide_in_nested_imports_or_macros() {
    for source in [
        "fn f() { let _ = std::env::var(\"X\"); }",
        "use std::{env as process, fs};",
        "use std::env::var as setting;",
        "fn f() { trace!(std::env::var_os(\"X\")); }",
    ] {
        let error = check_fixture(&fixture(&format!("{VALID}\n{source}"))).unwrap_err();
        assert!(error.contains("environment"), "{error}");
    }
}

#[test]
fn conditional_strictness_does_not_guarantee_a_strict_dto() {
    let dir = fixture(
        "#[cfg_attr(feature = \"strict\", serde(deny_unknown_fields))] pub struct Settings { pub gain: f32 }",
    );
    assert!(
        check_fixture(&dir)
            .unwrap_err()
            .contains("requires serde deny_unknown_fields")
    );
}
