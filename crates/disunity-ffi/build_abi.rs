//! The ABI surface: every exported function with its signature, every `#[repr(C)]` record with its
//! layout and every error reason, as canonical text. The build writes it to `abi/surface.g.txt`
//! and the ABI revision is the hash of that text, so the revision moves exactly when the surface
//! does and prose, formatting and the order of the source files never move it. It reads source
//! text only, so `build.rs` and the revision tests share it.

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
/// order, so the text does not depend on which file declares what.
pub fn surface<'a>(sources: impl IntoIterator<Item = &'a str>) -> String {
    let mut lines = Vec::new();
    for source in sources {
        lines.extend(
            exports(source)
                .into_iter()
                .map(|export| format!("fn {export}")),
        );
        lines.extend(records(source).iter().map(record_line));
        lines.extend(reasons(source));
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

/// Every `#[repr(C)]` record of `source`, with the offsets, size and alignment its C layout has.
pub fn records(source: &str) -> Vec<Record> {
    source.split("#[repr(C)]").skip(1).map(record).collect()
}

fn record(text: &str) -> Record {
    let name = text
        .split("pub struct ")
        .nth(1)
        .expect("named C record")
        .split_whitespace()
        .next()
        .expect("record name");
    let body = text
        .split('{')
        .nth(1)
        .expect("record body")
        .split('}')
        .next()
        .unwrap_or_default();
    let mut offset: usize = 0;
    let mut alignment = 1;
    let mut fields = Vec::new();
    for line in body
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| line.starts_with("pub "))
    {
        let (field, ty) = line
            .trim_start_matches("pub ")
            .split_once(':')
            .expect("scalar field");
        let ty = ty.trim().trim_end_matches(',');
        let (kind, length) = array_of(ty);
        let size = match kind {
            "u64" | "i64" | "f64" => 8,
            "u32" | "i32" | "f32" => 4,
            _ => panic!("unsupported C field {kind}"),
        };
        alignment = alignment.max(size);
        offset = offset.div_ceil(size) * size;
        fields.push(Field {
            name: field.trim().to_owned(),
            ty: canonical(ty),
            offset,
        });
        offset += size * length;
    }
    Record {
        name: name.to_owned(),
        fields,
        size: offset.div_ceil(alignment) * alignment,
        alignment,
    }
}

/// The element type and the length of a field type: a fixed array `[f32; 4]` is `f32` and 4, any
/// other type is itself and 1.
fn array_of(kind: &str) -> (&str, usize) {
    let Some(inner) = kind
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return (kind, 1);
    };
    let (element, length) = inner.split_once(';').expect("array length");
    (
        element.trim(),
        length.trim().parse().expect("a literal array length"),
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
fn exports(source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(mut current) = EXPORT_PREFIXES
            .iter()
            .find_map(|prefix| line.trim_start().strip_prefix(prefix))
        else {
            continue;
        };
        let mut text = String::new();
        let mut next = index + 1;
        loop {
            let code = current.split("//").next().unwrap_or_default();
            if let Some((head, _)) = code.split_once('{') {
                text.push_str(head);
                break;
            }
            text.push_str(code);
            text.push(' ');
            current = lines.get(next).expect("an export has a body");
            next += 1;
        }
        found.push(canonical(&text));
    }
    found
}

/// One `reason Name = tag` line for each variant of `DisunityErrorReason` in `source`.
fn reasons(source: &str) -> Vec<String> {
    let Some((_, rest)) = source.split_once("pub enum DisunityErrorReason {") else {
        return Vec::new();
    };
    let body = rest.split('}').next().unwrap_or_default();
    body.lines()
        .map(|line| line.split("//").next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (name, tag) = line.split_once('=').expect("an explicit error reason tag");
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
