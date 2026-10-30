//! The checked-in list of entry points (`exports.txt`) and the Rust code that exports them.
//!
//! One line per entry point, `<return type> <exported name>`, where the return type is the C#
//! binding's: `void`, `int`, `uint`, `IntPtr`, `float`, `byte`, `bool`, `ushort`, `ulong` or
//! `Vector3`. Blank lines and lines starting with `#` are ignored. `build.rs` and the unit
//! tests share this file.

use std::collections::BTreeSet;
use std::fmt::Write;

/// One exported function.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    /// The exported symbol, which is the binding's `EntryPoint`.
    pub name: String,
    /// The binding's return type.
    pub returns: String,
}

/// The Rust type of a return value and its zero.
fn rust_return(csharp: &str) -> Option<(&'static str, &'static str)> {
    Some(match csharp {
        "void" => ("()", ""),
        "int" | "bool" => ("i32", "0"),
        "uint" => ("u32", "0"),
        "IntPtr" => ("*mut core::ffi::c_void", "core::ptr::null_mut()"),
        "float" => ("f32", "0.0"),
        "byte" => ("u8", "0"),
        "ushort" => ("u16", "0"),
        "ulong" => ("u64", "0"),
        "Vector3" => ("Vector3", "Vector3 { x: 0.0, y: 0.0, z: 0.0 }"),
        _ => return None,
    })
}

/// Parses the list. A line that does not hold a known return type and a name, or that repeats
/// a name or a Rust function name, is an error that names its line number.
pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    let mut functions = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [returns, name] = fields[..] else {
            return Err(format!(
                "line {number}: expected `<return type> <name>`, found `{line}`"
            ));
        };
        if rust_return(returns).is_none() {
            return Err(format!("line {number}: unknown return type `{returns}`"));
        }
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("line {number}: `{name}` is not a C identifier"));
        }
        if !names.insert(name.to_owned()) {
            return Err(format!("line {number}: `{name}` is listed twice"));
        }
        if !functions.insert(function_name(name)) {
            return Err(format!(
                "line {number}: `{name}` has the same Rust name as an earlier entry"
            ));
        }
        entries.push(Entry {
            name: name.to_owned(),
            returns: returns.to_owned(),
        });
    }
    Ok(entries)
}

/// The Rust function name for an exported name: snake case, so `CSharp_AkAudioSettings_get`
/// becomes `c_sharp_ak_audio_settings_get`.
pub fn function_name(exported: &str) -> String {
    let chars: Vec<char> = exported.chars().collect();
    let mut name = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && index > 0 {
            let previous = chars[index - 1];
            let next_is_lower = chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            let boundary = previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_ascii_uppercase() && next_is_lower);
            if boundary && !name.ends_with('_') {
                name.push('_');
            }
        }
        if c != '_' || !name.ends_with('_') {
            name.push(c.to_ascii_lowercase());
        }
    }
    name
}

/// The Rust source that exports every entry. Each function ignores its arguments, which is
/// safe because the caller removes them from the stack on every supported C calling
/// convention. The source also holds `ENTRY_POINTS`, the exported names with their binding
/// return types, for tests.
pub fn generate(entries: &[Entry]) -> String {
    let mut code = String::from(
        "#[repr(C)]\npub(crate) struct Vector3 {\n    x: f32,\n    y: f32,\n    z: f32,\n}\n\n",
    );
    for entry in entries {
        let (rust, zero) = rust_return(&entry.returns).expect("parse checked the return type");
        let function = function_name(&entry.name);
        let name = &entry.name;
        let (returns, body) = if zero.is_empty() {
            (String::new(), String::new())
        } else {
            (format!(" -> {rust}"), format!("\n    {zero}\n"))
        };
        writeln!(
            code,
            "#[unsafe(export_name = \"{name}\")]\npub(crate) extern \"C\" fn {function}(){returns} {{{body}}}\n"
        )
        .expect("writing to a String cannot fail");
    }
    code.push_str("#[cfg(test)]\npub(crate) const ENTRY_POINTS: &[(&str, &str)] = &[\n");
    for entry in entries {
        writeln!(code, "    (\"{}\", \"{}\"),", entry.name, entry.returns)
            .expect("writing to a String cannot fail");
    }
    code.push_str("];\n");
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_without_a_name_is_rejected_with_its_number() {
        let error = parse("int CSharp_A\n\nuint\n").unwrap_err();
        assert!(error.starts_with("line 3:"), "{error}");
    }

    #[test]
    fn an_unknown_return_type_is_rejected_with_its_number() {
        let error = parse("# header\nint CSharp_A\ndouble CSharp_B\n").unwrap_err();
        assert!(error.starts_with("line 3:"), "{error}");
        assert!(error.contains("double"), "{error}");
    }

    #[test]
    fn a_repeated_name_is_rejected_with_the_second_line_number() {
        let error = parse("int CSharp_A\nvoid CSharp_B\nuint CSharp_A\n").unwrap_err();
        assert!(error.starts_with("line 3:"), "{error}");
    }

    #[test]
    fn two_names_with_one_rust_name_are_rejected() {
        let error = parse("int CSharp_AB\nint CSharp_Ab\n").unwrap_err();
        assert!(error.starts_with("line 2:"), "{error}");
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let entries = parse("# Wwise\n\n  int CSharp_A  \nVector3 CSharp_B\n").unwrap();
        assert_eq!(
            entries,
            vec![
                Entry {
                    name: "CSharp_A".into(),
                    returns: "int".into()
                },
                Entry {
                    name: "CSharp_B".into(),
                    returns: "Vector3".into()
                },
            ]
        );
    }

    #[test]
    fn exported_names_become_snake_case_function_names() {
        assert_eq!(
            function_name("CSharp_AkAudioSettings_uNumSamplesPerFrame_set"),
            "c_sharp_ak_audio_settings_u_num_samples_per_frame_set"
        );
        assert_eq!(
            function_name("CSharp_PostEventOnRoom__SWIG_6"),
            "c_sharp_post_event_on_room_swig_6"
        );
        assert_eq!(
            function_name("CSharp_AK_SOUNDBANK_VERSION_get"),
            "c_sharp_ak_soundbank_version_get"
        );
    }

    #[test]
    fn generated_code_exports_each_entry_with_the_binding_return_type() {
        let entries = parse("void CSharp_Stop\nbool CSharp_Ready\nIntPtr CSharp_Handle\n").unwrap();
        let code = generate(&entries);
        assert!(code.contains(
            "export_name = \"CSharp_Stop\")]\npub(crate) extern \"C\" fn c_sharp_stop() {}\n"
        ));
        assert!(code.contains("fn c_sharp_ready() -> i32 {\n    0\n}"));
        assert!(code.contains(
            "fn c_sharp_handle() -> *mut core::ffi::c_void {\n    core::ptr::null_mut()\n}"
        ));
    }
}
