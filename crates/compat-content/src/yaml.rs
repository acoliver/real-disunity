use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnityDocument {
    pub class_id: i32,
    pub file_id: i64,
    pub stripped: bool,
    pub kind: String,
    pub fields: Value,
}

/// Unity headers carry class/file identity outside the YAML data model.
/// Bodies are decoded by the YAML library, including flow collections and escapes.
pub fn parse_unity_yaml(text: &str, max_documents: usize) -> Result<Vec<UnityDocument>, String> {
    let mut documents = Vec::new();
    let mut header = None;
    let mut body = String::new();
    let mut ids = BTreeSet::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        if line.starts_with('%') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("--- !u!") {
            if let Some((class_id, file_id, stripped)) = header.take() {
                documents.push(decode(class_id, file_id, stripped, &body)?);
                body.clear();
            }
            if documents.len() >= max_documents {
                return Err("document count limit exceeded".into());
            }
            let parts: Vec<_> = rest.split_whitespace().collect();
            if !(2..=3).contains(&parts.len()) || (parts.len() == 3 && parts[2] != "stripped") {
                return Err(format!("invalid Unity document header: {line}"));
            }
            let class_id = parts[0].parse().map_err(|_| "invalid class ID")?;
            let file_id = parts[1]
                .strip_prefix('&')
                .ok_or("missing file ID anchor")?
                .parse()
                .map_err(|_| "invalid file ID")?;
            if !ids.insert(file_id) {
                return Err(format!("duplicate file ID {file_id}"));
            }
            header = Some((class_id, file_id, parts.len() == 3));
        } else if header.is_some() {
            body.push_str(line);
            body.push('\n');
        } else if !line.trim().is_empty() {
            return Err("content before Unity document header".into());
        }
    }
    if let Some((class_id, file_id, stripped)) = header {
        documents.push(decode(class_id, file_id, stripped, &body)?);
    }
    if documents.is_empty() {
        return Err("no Unity documents".into());
    }
    Ok(documents)
}

fn decode(
    class_id: i32,
    file_id: i64,
    stripped: bool,
    body: &str,
) -> Result<UnityDocument, String> {
    let value = parse_yaml_value(body)?;
    let map = value.as_object().ok_or("document must be a mapping")?;
    if map.len() != 1 {
        return Err("document must contain one type mapping".into());
    }
    let (kind, fields) = map.iter().next().ok_or("empty document")?;
    if !fields.is_object() {
        return Err("document fields must be a mapping".into());
    }
    Ok(UnityDocument {
        class_id,
        file_id,
        stripped,
        kind: kind.clone(),
        fields: fields.clone(),
    })
}

pub(crate) fn yaml_to_json(value: serde_yaml::Value, depth: usize) -> Result<Value, String> {
    use serde_yaml::Value as Y;
    if depth > 96 {
        return Err("YAML nesting limit exceeded".into());
    }
    Ok(match value {
        Y::Null => Value::Null,
        Y::Bool(v) => Value::Bool(v),
        Y::Number(v) => {
            if let Some(i) = v.as_i64() {
                Value::from(i)
            } else if let Some(i) = v.as_u64() {
                Value::from(i)
            } else {
                Value::Number(
                    serde_json::Number::from_f64(v.as_f64().ok_or("invalid float")?)
                        .ok_or("nonfinite float")?,
                )
            }
        }
        Y::String(v) => Value::String(v),
        Y::Sequence(v) => Value::Array(
            v.into_iter()
                .map(|v| yaml_to_json(v, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Y::Mapping(v) => {
            let mut map = serde_json::Map::new();
            for (key, value) in v {
                let key = match key {
                    Y::String(s) => s,
                    Y::Number(n) => n.to_string(),
                    _ => return Err("non-scalar mapping key".into()),
                };
                map.insert(key, yaml_to_json(value, depth + 1)?);
            }
            Value::Object(map)
        }
        Y::Tagged(_) => return Err("unexpected YAML tag in document body".into()),
    })
}

pub(crate) fn parse_yaml_value(text: &str) -> Result<Value, String> {
    let text = quote_guids(text);
    yaml_to_json(serde_yaml::from_str(&text).map_err(|e| e.to_string())?, 0)
}

// Unity's GUID scalar is always hexadecimal identity, including values that
// YAML's numeric resolver accepts as scientific notation. Quote only plain GUID
// mapping values; quoted text and literal/folded string bodies remain unchanged.
fn quote_guids(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut block_indent = None;
    let mut quote = None;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        if block_indent.is_some_and(|i| line.trim().is_empty() || indent > i) {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        block_indent = None;
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut previous = None;
        while i < bytes.len() {
            let c = bytes[i];
            if let Some(q) = quote {
                if let Some(end) = quoted_escape(line, i, q) {
                    out.push_str(&line[i..end]);
                    i = end;
                    continue;
                }
                if c == q {
                    quote = None;
                }
            } else if c == b'#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
                out.push_str(&line[i..]);
                break;
            } else if c == b'\'' || c == b'"' {
                quote = Some(c);
            } else if line[i..].starts_with("guid:")
                && (previous.is_none() || matches!(previous, Some(b'{' | b',')))
            {
                if let Some((start, end)) = guid_span(line, i) {
                    out.push_str(&line[i..start]);
                    out.push('"');
                    out.push_str(&line[start..end]);
                    out.push('"');
                    i = end;
                    previous = Some(b'"');
                    continue;
                }
            } else if matches!(c, b'|' | b'>') && previous == Some(b':') {
                block_indent = Some(indent);
            }
            // Advance by a whole UTF-8 scalar so user-authored names remain valid.
            let character = line[i..].chars().next().expect("nonempty line suffix");
            out.push(character);
            if !c.is_ascii_whitespace() {
                previous = Some(c);
            }
            i += character.len_utf8();
        }
        out.push('\n');
    }
    out
}

fn quoted_escape(line: &str, offset: usize, quote: u8) -> Option<usize> {
    let bytes = line.as_bytes();
    let current = bytes[offset];
    let escape = current == b'\\' && quote == b'"';
    let apostrophe = current == b'\'' && quote == b'\'' && bytes.get(offset + 1) == Some(&quote);
    (offset + 1 < bytes.len() && (escape || apostrophe)).then_some(offset + 2)
}

fn guid_span(line: &str, offset: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let begin = offset + 5;
    let start = begin
        + bytes[begin..]
            .iter()
            .position(|b| !b.is_ascii_whitespace())?;
    let end = (start..bytes.len())
        .find(|&i| !bytes[i].is_ascii_hexdigit())
        .unwrap_or(bytes.len());
    if end - start == 32
        && (end == bytes.len()
            || bytes[end].is_ascii_whitespace()
            || matches!(bytes[end], b',' | b'}' | b'#'))
    {
        Some((start, end))
    } else {
        None
    }
}
