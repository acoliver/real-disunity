//! The ABI surface: every exported function with its signature, every `#[repr(C)]` record with its
//! layout and every error reason, as canonical text. The build writes it to `abi/surface.g.txt`
//! and the ABI revision is the hash of that text, so the revision moves exactly when the surface
//! does and prose, formatting and the order of the source files never move it. It reads source
//! text only, so `build.rs` and the revision tests share it.
//!
//! Every failure message starts with the source file, then the struct or function it concerns.

/// A field of a `#[repr(C)]` record with its place in the C layout.
pub struct Field {
    pub name: String,
    /// The declared type, such as `u64` or `[f32; 4]`.
    pub ty: String,
    pub offset: usize,
}

/// A flat `#[repr(C)]` record of scalars and scalar arrays.
pub struct Record {
    pub name: String,
    pub fields: Vec<Field>,
    pub size: usize,
    pub alignment: usize,
}

const EXPORT_PREFIXES: [&str; 2] = ["pub unsafe extern \"C\" fn ", "pub extern \"C\" fn "];

/// The canonical surface of `sources`, one line per export, record and error reason in sorted
/// order, so the text does not depend on which file declares what. Each source is its file path
/// and its text; the path names every failure.
pub fn surface<'a>(sources: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut lines = Vec::new();
    for (file, source) in sources {
        lines.extend(
            exports(file, source)
                .into_iter()
                .map(|export| format!("fn {export}")),
        );
        lines.extend(records(file, source).iter().map(record_line));
        lines.extend(reasons(file, source));
    }
    lines.sort();
    let mut text = String::from(
        "// Disunity ABI surface. Generated from the exported functions, #[repr(C)] records and error\n// reasons of the ABI sources. Do not edit. The ABI revision is the FNV-1a hash of this file.\n",
    );
    for line in lines {
        text.push_str(&line);
        text.push('\n');
    }
    text
}

/// The code of a source line: everything before a line or doc comment.
fn code(line: &str) -> &str {
    line.split("//").next().unwrap_or_default()
}

/// How a record is laid out: `#[repr(C)]` with an optional `align(N)` or `packed`.
#[derive(Clone, Copy, Default)]
struct Repr {
    align: Option<usize>,
    packed: bool,
}

/// Parses the arguments of a `#[repr(...)]` attribute of the record `who` as a C layout.
fn parse_repr(who: &str, arguments: &str) -> Repr {
    let mut repr = Repr::default();
    let mut c = false;
    for argument in arguments.split(',').map(str::trim) {
        if argument == "C" {
            c = true;
        } else if argument == "packed" {
            repr.packed = true;
        } else if let Some(value) = argument
            .strip_prefix("align(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let align: usize = value
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("{who}: align({value}) must be a literal power of two"));
            assert!(
                align.is_power_of_two(),
                "{who}: align({value}) must be a power of two"
            );
            repr.align = Some(align);
        } else {
            panic!(
                "{who}: unsupported repr argument `{argument}` in `#[repr({arguments})]`; the ABI surface supports C, C with align(N) and C with packed"
            );
        }
    }
    assert!(
        c,
        "{who}: `#[repr({arguments})]` is not a C layout; the ABI surface supports only repr(C) records"
    );
    assert!(
        !(repr.packed && repr.align.is_some()),
        "{who}: `#[repr({arguments})]` combines packed and align, which the ABI surface does not support"
    );
    repr
}

/// A record found in a source: its name, layout attribute, the text of its body and the source
/// lines from its attribute to its closing brace.
struct Scanned {
    who: String,
    name: String,
    repr: Repr,
    body: String,
    verbatim: String,
}

/// Every `#[repr(C)]`, `#[repr(C, align(N))]` and `#[repr(C, packed)]` struct of `source`. A `repr`
/// named in a comment is not an attribute. A struct with any other `repr` fails the build, since
/// its layout is not computed.
fn scan(file: &str, source: &str) -> Vec<Scanned> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let start = index;
        let line = code(lines[index]).trim_start();
        index += 1;
        let Some(rest) = line.strip_prefix("#[repr(") else {
            continue;
        };
        let mut attribute = rest.to_owned();
        while !attribute.contains(")]") && index < lines.len() {
            attribute.push(' ');
            attribute.push_str(code(lines[index]).trim());
            index += 1;
        }
        let arguments = attribute
            .rsplit_once(")]")
            .unwrap_or_else(|| panic!("{file}: unterminated `#[repr(` attribute"))
            .0
            .to_owned();
        // The item the attribute belongs to: the first line after the attributes that names one.
        let mut cursor = index;
        let mut item = None;
        while cursor < lines.len() {
            let text = code(lines[cursor]).trim();
            cursor += 1;
            if text.is_empty() || text.starts_with("#[") {
                continue;
            }
            item = Some(text);
            break;
        }
        let Some((_, after)) = item.and_then(|item| item.split_once("struct ")) else {
            // `#[repr(u64)] enum` and the like are not records.
            continue;
        };
        let name = after
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        assert!(
            !name.is_empty(),
            "{file}: a record under `#[repr({arguments})]` has no name"
        );
        let who = format!("{file}: {name}");
        let repr = parse_repr(&who, &arguments);
        let mut end = cursor - 1;
        while !code(lines[end]).contains('}') {
            end += 1;
            assert!(end < lines.len(), "{who}: the record body is never closed");
        }
        let body: Vec<&str> = lines[cursor - 1..=end]
            .iter()
            .map(|line| code(line))
            .collect();
        found.push(Scanned {
            who,
            name: name.to_owned(),
            repr,
            body: body.join("\n"),
            verbatim: lines[start..=end].join("\n"),
        });
        index = end + 1;
    }
    found
}

/// Every C record of `source`, with the offsets, size and alignment its layout has.
pub fn records(file: &str, source: &str) -> Vec<Record> {
    scan(file, source)
        .iter()
        .map(|found| record(&found.who, &found.name, found.repr, &found.body))
        .collect()
}

/// The source text of every C record of `source`, from its attribute to its closing brace.
pub fn record_sources(file: &str, source: &str) -> Vec<String> {
    scan(file, source)
        .into_iter()
        .map(|found| found.verbatim)
        .collect()
}

fn record(who: &str, name: &str, repr: Repr, text: &str) -> Record {
    let body = text
        .split('{')
        .nth(1)
        .unwrap_or_else(|| panic!("{who}: a record has a braced body"))
        .split('}')
        .next()
        .unwrap_or_default();
    let mut offset: usize = 0;
    let mut alignment = 1;
    let mut fields = Vec::new();
    for line in body
        .lines()
        .map(|line| code(line).trim())
        .filter(|line| line.starts_with("pub "))
    {
        let (field, ty) = line
            .trim_start_matches("pub ")
            .split_once(':')
            .unwrap_or_else(|| panic!("{who}: field `{line}` is not `name: type`"));
        let ty = ty.trim().trim_end_matches(',');
        let (kind, length) = array_of(who, ty);
        let size = match kind {
            "u64" | "i64" | "f64" => 8,
            "u32" | "i32" | "f32" => 4,
            _ => panic!(
                "{who}: field `{}` has unsupported C type `{kind}`",
                field.trim()
            ),
        };
        alignment = alignment.max(size);
        if !repr.packed {
            offset = offset.div_ceil(size) * size;
        }
        fields.push(Field {
            name: field.trim().to_owned(),
            ty: canonical(ty),
            offset,
        });
        offset += size * length;
    }
    let (size, alignment) = if repr.packed {
        (offset, 1)
    } else if let Some(align) = repr.align {
        assert!(
            align >= alignment,
            "{who}: align({align}) is below the natural alignment {alignment} of its fields"
        );
        (offset.div_ceil(align) * align, align)
    } else {
        (offset.div_ceil(alignment) * alignment, alignment)
    };
    Record {
        name: name.to_owned(),
        fields,
        size,
        alignment,
    }
}

/// The element type and the length of a field type: a fixed array `[f32; 4]` is `f32` and 4, any
/// other type is itself and 1.
fn array_of<'a>(who: &str, kind: &'a str) -> (&'a str, usize) {
    let Some(inner) = kind
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return (kind, 1);
    };
    let (element, length) = inner
        .split_once(';')
        .unwrap_or_else(|| panic!("{who}: array type `{kind}` has no length"));
    (
        element.trim(),
        length
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("{who}: array type `{kind}` needs a literal length")),
    )
}

fn record_line(record: &Record) -> String {
    let fields: Vec<String> = record
        .fields
        .iter()
        .map(|field| format!("{}: {} @{}", field.name, field.ty, field.offset))
        .collect();
    format!(
        "record {}: size {}, align {} {{ {} }}",
        record.name,
        record.size,
        record.alignment,
        fields.join(", ")
    )
}

/// The canonical `name(arguments) -> result` of every exported function of `source`. The
/// signature runs from the `fn` keyword to the opening brace of the body.
pub fn exports(file: &str, source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(mut current) = EXPORT_PREFIXES
            .iter()
            .find_map(|prefix| line.trim_start().strip_prefix(prefix))
        else {
            continue;
        };
        let name = current
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        let who = format!("{file}: {name}");
        let mut text = String::new();
        let mut next = index + 1;
        loop {
            let code = code(current);
            if let Some((head, _)) = code.split_once('{') {
                text.push_str(head);
                break;
            }
            text.push_str(code);
            text.push(' ');
            current = lines
                .get(next)
                .unwrap_or_else(|| panic!("{who}: an export has a body, but the file ends first"));
            next += 1;
        }
        found.push(canonical(&text));
    }
    found
}

/// One `reason Name = tag` line for each variant of `DisunityErrorReason` in `source`.
fn reasons(file: &str, source: &str) -> Vec<String> {
    let Some((_, rest)) = source.split_once("pub enum DisunityErrorReason {") else {
        return Vec::new();
    };
    let body = rest.split('}').next().unwrap_or_default();
    body.lines()
        .map(|line| code(line).trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, tag) = line.split_once('=').unwrap_or_else(|| {
                panic!("{file}: DisunityErrorReason: `{line}` has no explicit error reason tag")
            });
            format!(
                "reason {} = {}",
                name.trim(),
                tag.trim().trim_end_matches(',')
            )
        })
        .collect()
}

/// `text` with its whitespace and a trailing comma before `)` normalized, so the layout of the
/// source never shows in the surface.
fn canonical(text: &str) -> String {
    let tokens = tokens(text);
    let kept: Vec<&str> = tokens
        .iter()
        .enumerate()
        .filter(|(index, token)| {
            !(token.as_str() == "," && tokens.get(index + 1).is_some_and(|next| next == ")"))
        })
        .map(|(_, token)| token.as_str())
        .collect();
    let mut out = String::new();
    let mut previous = "";
    for token in kept {
        let spaced = (is_word(previous) && is_word(token))
            || matches!(previous, "," | ":" | ";" | "->")
            || token == "->";
        if spaced {
            out.push(' ');
        }
        out.push_str(token);
        previous = token;
    }
    out
}

fn is_word(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|first| first.is_alphanumeric() || first == '_' || first == '"')
}

/// Words, string literals, `->`, `::` and single punctuation characters, without whitespace.
fn tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(first) = chars.next() {
        if first.is_whitespace() {
            continue;
        }
        let mut token = String::from(first);
        if first.is_alphanumeric() || first == '_' {
            while let Some(next) = chars.next_if(|next| next.is_alphanumeric() || *next == '_') {
                token.push(next);
            }
        } else if first == '"' {
            for next in chars.by_ref() {
                token.push(next);
                if next == '"' {
                    break;
                }
            }
        } else if let Some(second) = chars
            .next_if(|second| (first == '-' && *second == '>') || (first == ':' && *second == ':'))
        {
            token.push(second);
        }
        tokens.push(token);
    }
    tokens
}
