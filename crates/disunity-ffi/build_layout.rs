//! Layout expectations derived from the scalar C ABI records, independently of csbindgen.
use std::fmt::Write;
use std::path::Path;

pub fn generate(out: &Path) {
    let mut managed = String::from(
        "// Generated from the native C ABI records. Do not edit.\nusing System;\nusing System.Runtime.InteropServices;\nnamespace Disunity.Native;\ninternal static class NativeLayouts\n{\n    internal static void Verify()\n    {\n",
    );
    let mut native = String::new();
    for file in record_sources() {
        let module = if file == "lib" {
            "crate".to_owned()
        } else {
            format!("crate::{file}")
        };
        let source = std::fs::read_to_string(format!("src/{file}.rs")).expect("ABI source");
        for record in source.split("#[repr(C)]").skip(1) {
            append_record(record, &module, &mut managed, &mut native);
        }
    }
    managed.push_str(&format!(
        "    }}\n}}\n/// The native ABI revision this assembly was built against.\ninternal static class NativeAbi\n{{\n    internal const uint Version = {};\n}}\n",
        crate::abi_version()
    ));
    std::fs::write(out.join("NativeLayouts.g.cs"), &managed).expect("managed layout expectations");
    std::fs::write(out.join("NativeLayouts.g.rs"), native).expect("native layout expectations");
    let checked =
        Path::new(&std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"))
            .join("../../csharp/Disunity/NativeLayouts.g.cs");
    println!("cargo:rerun-if-changed={}", checked.display());
    assert!(
        std::fs::read_to_string(&checked).unwrap_or_default() == managed,
        "generated layouts drifted; copy {} to {}",
        out.join("NativeLayouts.g.cs").display(),
        checked.display()
    );
}

/// Every bound source, so a file that declares C records cannot be left out. Files keep the
/// order their records were first generated in, so existing output does not move.
fn record_sources() -> Vec<&'static str> {
    const FIRST: [&str; 13] = [
        "abi",
        "lib",
        "scheduling",
        "buffers",
        "cloning",
        "callbacks",
        "input_axes",
        "physics",
        "physics_materials",
        "ui",
        "sprites",
        "presentation",
        "object_api",
    ];
    let mut files: Vec<&str> = crate::ABI_SOURCES
        .iter()
        .map(|path| path.trim_start_matches("src/").trim_end_matches(".rs"))
        .collect();
    files.sort_by_key(|file| {
        FIRST
            .iter()
            .position(|first| first == file)
            .unwrap_or(FIRST.len())
    });
    files
}

fn append_record(record: &str, module: &str, managed: &mut String, native: &mut String) {
    let name = record
        .split("pub struct ")
        .nth(1)
        .expect("named C record")
        .split_whitespace()
        .next()
        .expect("record name");
    let function = name.to_lowercase();
    writeln!(native, "#[test]\nfn {function}() {{").unwrap();
    let fields = record
        .split('{')
        .nth(1)
        .expect("record body")
        .split('}')
        .next()
        .unwrap();
    let mut offset: usize = 0;
    let mut alignment = 1;
    let mut count = 0;
    let mut actual_offsets = Vec::new();
    let mut expected_offsets = Vec::new();
    for line in fields
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub "))
    {
        let (field, kind) = line
            .trim_start_matches("pub ")
            .split_once(':')
            .expect("scalar field");
        let kind = kind.trim().trim_end_matches(',');
        let size = match kind {
            "u64" | "f64" => 8,
            "u32" | "i32" | "f32" => 4,
            _ => panic!("unsupported C field {kind}"),
        };
        alignment = alignment.max(size);
        offset = offset.div_ceil(size) * size;
        writeln!(managed, "        if (Marshal.OffsetOf<{name}>(\"{field}\").ToInt32() != {offset}) throw new InvalidOperationException(\"{name}.{field} offset\");").unwrap();
        actual_offsets.push(format!("std::mem::offset_of!({module}::{name}, {field})"));
        expected_offsets.push(offset.to_string());
        offset += size;
        count += 1;
    }
    writeln!(
        native,
        "    assert_eq!([{}], [{}]);",
        actual_offsets.join(", "),
        expected_offsets.join(", ")
    )
    .unwrap();
    let size = offset.div_ceil(alignment) * alignment;
    writeln!(managed, "        if (Marshal.SizeOf<{name}>() != {size} || typeof({name}).GetFields().Length != {count}) throw new InvalidOperationException(\"{name} layout\");").unwrap();
    writeln!(native, "    assert_eq!(std::mem::size_of::<{module}::{name}>(), {size});\n    assert_eq!(std::mem::align_of::<{module}::{name}>(), {alignment});").unwrap();
    native.push_str("}\n");
}
