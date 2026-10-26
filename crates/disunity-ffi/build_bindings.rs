use std::path::{Path, PathBuf};

pub fn generate(output: &Path) {
    let mut builder = csbindgen::Builder::default();
    for file in super::ABI_SOURCES {
        builder = builder.input_extern_file(file);
    }
    let full = output.join("NativeBindings.full.g.cs");
    builder
        .csharp_dll_name("worldrunner_disunity_ffi")
        .csharp_namespace("Disunity.Native")
        .csharp_class_name("NativeBindings")
        .csharp_class_accessibility("internal")
        .generate_csharp_file(&full)
        .expect("generate managed ABI bindings");
    let source = std::fs::read_to_string(full).expect("read generated bindings");
    let (imports, records) = source
        .split_once("\n    }\n")
        .expect("generated import class");
    let (header, _) = source
        .split_once("    internal static unsafe partial class")
        .expect("generated namespace header");
    let imports = imports
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let imports = format!("{imports}\n    }}\n}}\n");
    let records = format!("{header}{records}");
    let generated = [
        ("NativeBindings.g.cs", imports),
        ("NativeRecords.g.cs", records),
    ];
    for (name, contents) in &generated {
        assert!(
            contents.lines().count() < 750,
            "split generated file {name}"
        );
        std::fs::write(output.join(name), contents).expect("write split bindings");
    }
    for (name, _) in generated {
        check(output, name);
    }
}

fn check(output: &Path, name: &str) {
    let generated = output.join(name);
    let checked = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"),
    )
    .join("../../csharp/Disunity")
    .join(name);
    println!("cargo:rerun-if-changed={}", checked.display());
    assert!(
        std::fs::read(&checked).unwrap_or_default()
            == std::fs::read(&generated).expect("generated bindings"),
        "managed ABI bindings drifted; the checked file was NOT modified. \
         Review the ABI change, then explicitly copy {} to {}. \
         See csharp/README.md for the regeneration workflow",
        generated.display(),
        checked.display()
    );
}
