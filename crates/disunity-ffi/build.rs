//! Generates bindings in Cargo's output directory and rejects checked-in ABI drift.

use std::path::PathBuf;

fn main() {
    let checked = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"),
    )
    .join("../../csharp/Disunity/NativeBindings.g.cs");
    for input in ["src/abi.rs", "src/lib.rs", "build.rs"] {
        println!("cargo:rerun-if-changed={input}");
    }
    println!("cargo:rerun-if-changed={}", checked.display());

    let generated = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("NativeBindings.g.cs");
    csbindgen::Builder::default()
        .input_extern_file("src/abi.rs")
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("worldrunner_disunity_ffi")
        .csharp_namespace("Disunity")
        .csharp_class_name("NativeBindings")
        .csharp_class_accessibility("internal")
        .generate_csharp_file(&generated)
        .expect("generate managed ABI bindings");

    let expected = std::fs::read(&generated).expect("read generated bindings");
    let actual = std::fs::read(&checked).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}; generated bindings: {}",
            checked.display(),
            generated.display()
        )
    });
    assert!(
        actual == expected,
        "managed ABI bindings drifted; the checked file was NOT modified. \
         Review the ABI change, then explicitly copy {} to {}. \
         See csharp/README.md for the regeneration workflow",
        generated.display(),
        checked.display()
    );
}
