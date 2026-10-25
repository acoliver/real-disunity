use std::fmt::Write;
use std::path::Path;

pub fn generate(out: &Path) {
    let source = std::fs::read_to_string("src/abi.rs").expect("ABI reasons");
    let body = source
        .split("pub enum DisunityErrorReason {")
        .nth(1)
        .expect("reason enum")
        .split('}')
        .next()
        .unwrap();
    let mut managed = String::from(
        "// Generated from the native ABI reasons. Do not edit.\nnamespace Disunity.Hosting;\npublic enum DisunityErrorReason : ulong\n{",
    );
    managed.push_str(body);
    managed.push_str("}\n");
    let scheduling = std::fs::read_to_string("src/scheduling.rs").expect("diagnostic bound");
    let limit = scheduling
        .split("pub const MAX_DIAGNOSTIC_BYTES: usize = ")
        .nth(1)
        .expect("bound")
        .split(';')
        .next()
        .unwrap();
    writeln!(
        managed,
        "internal static class NativeDiagnosticLimits {{ internal const int MaxBytes = {limit}; }}"
    )
    .unwrap();
    let checked = Path::new("../../csharp/Disunity/NativeErrors.g.cs");
    std::fs::write(out.join("NativeErrors.g.cs"), &managed).expect("generated reasons");
    println!("cargo:rerun-if-changed={}", checked.display());
    assert_eq!(
        std::fs::read_to_string(checked).unwrap_or_default(),
        managed,
        "generated reasons drifted; copy {} to {}",
        out.join("NativeErrors.g.cs").display(),
        checked.display()
    );
}
