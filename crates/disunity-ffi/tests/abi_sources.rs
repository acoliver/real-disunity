//! `build.rs` derives the ABI surface from the files in `ABI_SOURCES`. A source file that declares
//! exports or C records but is missing from that list would change the ABI without changing its
//! revision, so the list must name every such file.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The `src/...` paths in the `ABI_SOURCES` constant of `build.rs`.
fn listed_sources() -> BTreeSet<String> {
    let build = std::fs::read_to_string(manifest_dir().join("build.rs")).expect("read build.rs");
    let list = build
        .split("const ABI_SOURCES: &[&str] = &[")
        .nth(1)
        .expect("build.rs declares ABI_SOURCES")
        .split("];")
        .next()
        .expect("ABI_SOURCES ends");
    list.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// A file declares ABI when a line of code (not a comment) holds an `extern "C" fn` or a `repr(C`
/// attribute. Test modules (`*_tests.rs`) hold test-only callbacks and layouts, not the ABI.
fn declares_abi(text: &str) -> bool {
    text.lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .any(|code| code.contains("extern \"C\" fn") || code.contains("repr(C"))
}

fn sources_declaring_abi() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(manifest_dir().join("src")).expect("read src") {
        let path = entry.expect("directory entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("utf-8 file name")
            .to_owned();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            || name.ends_with("_tests.rs")
        {
            continue;
        }
        if declares_abi(&std::fs::read_to_string(&path).expect("read a source")) {
            found.insert(format!("src/{name}"));
        }
    }
    found
}

#[test]
fn abi_sources_lists_every_file_that_declares_exports_or_c_records() {
    let listed = listed_sources();
    let missing: Vec<_> = sources_declaring_abi()
        .difference(&listed)
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "these files declare `extern \"C\" fn` or `repr(C` but are not in ABI_SOURCES in build.rs: {missing:?}"
    );
}

#[test]
fn abi_sources_names_only_files_that_exist() {
    for file in listed_sources() {
        assert!(
            manifest_dir().join(&file).is_file(),
            "ABI_SOURCES names {file}, which does not exist"
        );
    }
}

#[test]
fn the_guard_notices_a_file_that_declares_abi() {
    assert!(declares_abi("pub unsafe extern \"C\" fn disunity_x() {}"));
    assert!(declares_abi("#[repr(C, align(16))]\npub struct A {}"));
    assert!(!declares_abi(
        "// extern \"C\" fn x and repr(C) in a comment\nfn y() {}"
    ));
}
