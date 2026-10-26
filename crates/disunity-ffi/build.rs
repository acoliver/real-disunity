//! Generates bindings in Cargo's output directory and rejects checked-in ABI drift.

use std::path::PathBuf;
mod build_errors;
mod build_layout;

fn main() {
    let checked = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"),
    )
    .join("../../csharp/Disunity/NativeBindings.g.cs");
    for input in [
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
        "src/hierarchy.rs",
        "src/callbacks.rs",
        "src/input.rs",
        "src/physics.rs",
        "src/physics_materials.rs",
        "src/ui.rs",
        "src/object_api.rs",
        "build.rs",
        "build_layout.rs",
        "build_errors.rs",
        "src/errors.rs",
    ] {
        println!("cargo:rerun-if-changed={input}");
    }
    println!("cargo:rerun-if-changed={}", checked.display());

    let generated = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("NativeBindings.g.cs");
    generate_bindings(&generated);
    split_managed_records(&generated, &checked);

    build_errors::generate(generated.parent().expect("output directory"));
    generate_native_records();
    build_layout::generate(generated.parent().expect("output directory"));
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

fn generate_native_records() {
    let mut output =
        String::from("// Generated from disunity-ffi ABI definitions. Do not edit.\n\n");
    for file in [
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
        "src/hierarchy.rs",
        "src/callbacks.rs",
        "src/input.rs",
        "src/physics.rs",
        "src/physics_materials.rs",
        "src/ui.rs",
        "src/object_api.rs",
    ] {
        let source = std::fs::read_to_string(file).expect("read ABI definitions");
        for record in source.split("#[repr(C)]").skip(1) {
            let end = record.find("\n}").expect("flat C record ends") + 2;
            output.push_str("#[repr(C)]");
            output.push_str(&record[..end]);
            output.push_str("\n\n");
        }
    }
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

fn generate_bindings(generated: &std::path::Path) {
    csbindgen::Builder::default()
        .input_extern_file("src/abi.rs")
        .input_extern_file("src/assets.rs")
        .input_extern_file("src/camera_layers.rs")
        .input_extern_file("src/camera_data.rs")
        .input_extern_file("src/sprites.rs")
        .input_extern_file("src/presentation.rs")
        .input_extern_file("src/lib.rs")
        .input_extern_file("src/scheduling.rs")
        .input_extern_file("src/buffers.rs")
        .input_extern_file("src/cloning.rs")
        .input_extern_file("src/hierarchy.rs")
        .input_extern_file("src/callbacks.rs")
        .input_extern_file("src/input.rs")
        .input_extern_file("src/physics.rs")
        .input_extern_file("src/physics_materials.rs")
        .input_extern_file("src/ui.rs")
        .input_extern_file("src/object_api.rs")
        .csharp_dll_name("worldrunner_disunity_ffi")
        .csharp_namespace("Disunity.Native")
        .csharp_class_name("NativeBindings")
        .csharp_class_accessibility("internal")
        .generate_csharp_file(generated)
        .expect("generate managed ABI bindings");
}

fn split_managed_records(generated: &std::path::Path, checked: &std::path::Path) {
    let source = std::fs::read_to_string(generated).expect("read generated bindings");
    let class_end = source
        .find("\n    }\n")
        .expect("generated import class end")
        + 7;
    let header_end = source
        .find("    internal static unsafe partial class NativeBindings")
        .expect("generated import class");
    let records = format!("{}{}", &source[..header_end], &source[class_end..]);
    let output = generated.with_file_name("NativeRecords.g.cs");
    std::fs::write(&output, records.as_bytes()).expect("write generated records");
    let tracked = checked.with_file_name("NativeRecords.g.cs");
    println!("cargo:rerun-if-changed={}", tracked.display());
    assert_eq!(
        std::fs::read(&tracked).unwrap_or_default(),
        records.as_bytes(),
        "managed record bindings drifted; copy {} to {}",
        output.display(),
        tracked.display()
    );
    std::fs::write(generated, format!("{}\n}}\n", &source[..class_end]))
        .expect("write generated imports");
}
