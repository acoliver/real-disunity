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
    let mut generated = import_files(imports);
    generated.push((
        "NativeRecords.g.cs".to_owned(),
        format!("{header}{records}"),
    ));
    for (name, contents) in &generated {
        assert!(
            contents.lines().count() < LIMIT,
            "split generated file {name}"
        );
        std::fs::write(output.join(name), contents).expect("write split bindings");
    }
    for (name, _) in &generated {
        check(output, name);
    }
}

/// Source files stay below this many lines.
const LIMIT: usize = 750;

/// Packs the import declarations, in generated order, into as many files as stay below
/// [`LIMIT`]: `NativeBindings.g.cs`, then `NativeBindings.2.g.cs` and so on. Each file holds
/// part of the same partial class.
fn import_files(imports: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = imports
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let open = lines
        .iter()
        .position(|line| line.contains("const string __DllName"))
        .expect("generated library name")
        + 1;
    let (header, body) = lines.split_at(open);
    let class = &header[..header.len() - 1];
    let mut files = vec![header.to_vec()];
    let mut declaration = Vec::new();
    for line in body {
        declaration.push(*line);
        if line.contains("internal static extern") {
            let last = files.last().expect("first file");
            if last.len() + declaration.len() + 2 >= LIMIT {
                files.push(class.to_vec());
            }
            files.last_mut().expect("file").append(&mut declaration);
        }
    }
    assert!(
        declaration.is_empty(),
        "generated import without a declaration"
    );
    files
        .into_iter()
        .enumerate()
        .map(|(index, lines)| {
            let name = if index == 0 {
                "NativeBindings.g.cs".to_owned()
            } else {
                format!("NativeBindings.{}.g.cs", index + 1)
            };
            (name, format!("{}\n    }}\n}}\n", lines.join("\n")))
        })
        .collect()
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
