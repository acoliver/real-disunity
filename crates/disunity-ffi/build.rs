//! Generates bindings in Cargo's output directory and rejects checked-in ABI drift.

use std::path::PathBuf;
mod build_bindings;
mod build_errors;
mod build_layout;

const ABI_SOURCES: &[&str] = &[
    "src/abi.rs",
    "src/assets.rs",
    "src/camera_layers.rs",
    "src/camera_data.rs",
    "src/sprites.rs",
    "src/presentation.rs",
    "src/lib.rs",
    "src/scheduling.rs",
    "src/buffers.rs",
    "src/cloning.rs",
    "src/construction.rs",
    "src/hierarchy.rs",
    "src/callbacks.rs",
    "src/input.rs",
    "src/input_axes.rs",
    "src/physics.rs",
    "src/physics_materials.rs",
    "src/ui.rs",
    "src/object_api.rs",
];

fn main() {
    for input in ABI_SOURCES.iter().copied().chain([
        "build.rs",
        "build_bindings.rs",
        "build_layout.rs",
        "build_errors.rs",
        "src/errors.rs",
    ]) {
        println!("cargo:rerun-if-changed={input}");
    }
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    build_bindings::generate(&output);
    build_errors::generate(&output);
    generate_native_records();
    build_layout::generate(&output);
}

fn generate_native_records() {
    let mut output =
        String::from("// Generated from disunity-ffi ABI definitions. Do not edit.\n\n");
    for file in ABI_SOURCES {
        let source = std::fs::read_to_string(file).expect("read ABI definitions");
        for record in source.split("#[repr(C)]").skip(1) {
            let end = record.find("\n}").expect("flat C record ends") + 2;
            output.push_str("#[repr(C)]");
            output.push_str(&record[..end]);
            output.push_str("\n\n");
        }
    }
    output.push_str(&format!(
        "/// The ABI revision these records belong to.
pub const DISUNITY_ABI_VERSION: u32 = {};

",
        abi_version()
    ));
    let source = std::fs::read_to_string("src/abi.rs").expect("ABI reasons");
    let reason = source.split("#[repr(u64)]").nth(1).expect("reason enum");
    let end = reason.find("\n}").expect("enum end") + 2;
    output.push_str("#[repr(u64)]");
    output.push_str(&reason[..end]);
    output.push('\n');
    let path = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("NativeBindings.g.rs");
    std::fs::write(&path, output.as_bytes()).expect("write native records");
    let checked = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"),
    )
    .join("../disunity-runtime/src/native.g.rs");
    println!("cargo:rerun-if-changed={}", checked.display());
    assert_eq!(
        std::fs::read(&checked).unwrap_or_default(),
        output.as_bytes(),
        "native ABI bindings drifted; copy {} to {}",
        path.display(),
        checked.display()
    );
}

/// The revision declared by `DISUNITY_ABI_VERSION` in `src/abi.rs`, so generated consumers never
/// repeat the number.
pub(crate) fn abi_version() -> u32 {
    let source = std::fs::read_to_string("src/abi.rs").expect("ABI version source");
    let line = source
        .lines()
        .find_map(|line| line.strip_prefix("pub const DISUNITY_ABI_VERSION: u32 = "))
        .expect("ABI version constant");
    line.trim_end_matches(';')
        .parse()
        .expect("numeric ABI version")
}
