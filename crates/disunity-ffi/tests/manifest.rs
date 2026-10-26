//! The manifest is the contract: every exported symbol is listed with the
//! version that introduced it, and the generated C# bindings expose exactly
//! the same surface. These tests fail the build when any of the three drift.

use std::collections::BTreeSet;
use std::path::Path;

/// Every Rust source under `src/`, so exports in new files are covered without a list here.
fn sources(directory: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(std::fs::read_to_string(&path).expect("read source"));
        }
    }
}

/// Exports declared anywhere in the crate's sources.
fn declared_exports() -> BTreeSet<String> {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut names = BTreeSet::new();
    for line in files.iter().flat_map(|file| file.lines()) {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("pub unsafe extern \"C\" fn ")
            .or_else(|| line.strip_prefix("pub extern \"C\" fn "))
        {
            let name = rest.split('(').next().unwrap_or_default().trim().to_owned();
            assert!(!name.is_empty(), "unparsed export line: {line}");
            names.insert(name);
        }
    }
    assert!(!names.is_empty(), "no exports found; parser is stale");
    names
}

fn manifest() -> serde_json::Value {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("disunity_interface.json"),
    )
    .expect("disunity_interface.json is tracked beside the crate");
    serde_json::from_str(&text).expect("manifest parses as JSON")
}

fn generated_bindings() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../csharp/Disunity/NativeBindings.g.cs"),
    )
    .expect("csbindgen output is generated into csharp/Disunity and tracked")
}

#[test]
fn every_export_is_listed_in_the_manifest() {
    let listed: BTreeSet<String> = manifest()["functions"]
        .as_array()
        .expect("functions is an array")
        .iter()
        .map(|entry| entry["name"].as_str().expect("name").to_owned())
        .collect();
    let declared = declared_exports();
    let missing: Vec<_> = declared.difference(&listed).collect();
    let extra: Vec<_> = listed.difference(&declared).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "manifest drift: missing {missing:?}, extra {extra:?}"
    );
}

#[test]
fn every_manifest_entry_records_since_and_result() {
    for entry in manifest()["functions"]
        .as_array()
        .expect("functions is an array")
    {
        let since = entry["since"].as_str().unwrap_or_default();
        assert!(
            since.matches('.').count() == 2 && !since.is_empty(),
            "since must be a full version, got {since:?}"
        );
        assert_eq!(
            entry["rets"].as_str(),
            Some("DisunityResult"),
            "every export returns DisunityResult"
        );
    }
}

#[test]
fn manifest_abi_version_matches_the_library_constant() {
    let manifest_version = manifest()["abi_version"].as_u64().expect("u64") as u32;
    assert_eq!(
        manifest_version,
        worldrunner_disunity_ffi::DISUNITY_ABI_VERSION
    );
}

#[test]
fn checked_bindings_match_complete_generated_signatures_and_layouts() {
    let generated = std::fs::read(Path::new(env!("OUT_DIR")).join("NativeBindings.g.cs"))
        .expect("the build generates bindings in OUT_DIR without changing the checked file");
    assert_eq!(generated, generated_bindings().as_bytes());
}

#[test]
fn checked_managed_records_match_the_complete_generated_records() {
    let generated = std::fs::read(Path::new(env!("OUT_DIR")).join("NativeRecords.g.cs"))
        .expect("build generates managed records without changing checked sources");
    let checked = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../csharp/Disunity/NativeRecords.g.cs"),
    )
    .expect("generated managed record declarations are tracked");
    assert_eq!(
        checked, generated,
        "complete generated record declarations drifted"
    );
}

#[test]
fn checked_bindings_import_every_export() {
    let bindings = generated_bindings();
    let imported: BTreeSet<String> = bindings
        .split("EntryPoint = \"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap_or_default().to_owned())
        .collect();
    let declared = declared_exports();
    let missing: Vec<_> = declared.difference(&imported).collect();
    let extra: Vec<_> = imported.difference(&declared).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "bindings drift (list new ABI files in build.rs EXTERN_INPUTS): missing {missing:?}, extra {extra:?}"
    );
}
