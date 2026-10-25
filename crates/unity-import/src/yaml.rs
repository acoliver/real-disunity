//! Unity's serialized YAML subset.
//!
//! Unity writes `%` directives, `--- !u!CLASS &FILEID` document headers (optionally followed by
//! `stripped`), block mappings, block sequences (including sequences written at their parent
//! key's indentation), flow collections, and plain, single-quoted and double-quoted scalars.
//! Anchors, aliases, tags inside documents, block scalars and multi-document streams without
//! Unity headers are outside this subset and are rejected.

mod block;
mod flow;
mod node;
mod scalar;

use std::collections::BTreeSet;

pub use node::{Mapping, Node, Scalar, ScalarStyle};

/// Bounds applied while parsing untrusted project files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_documents: usize,
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_documents: 262_144,
            max_depth: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum YamlError {
    #[error("line {line}: {message}")]
    Syntax { line: usize, message: String },
    #[error("{0} limit exceeded")]
    Limit(&'static str),
}

impl YamlError {
    pub(crate) fn syntax(line: usize, message: impl Into<String>) -> Self {
        Self::Syntax {
            line,
            message: message.into(),
        }
    }
}

/// One `--- !u!CLASS &FILEID` document: a single object of a Unity class.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub class_id: i32,
    pub file_id: i64,
    /// Stripped documents only name an object inside a prefab instance.
    pub stripped: bool,
    pub type_name: String,
    pub body: Mapping,
}

struct Header {
    class_id: i32,
    file_id: i64,
    stripped: bool,
    line: usize,
}

/// Parses a Unity scene, prefab or asset file into its documents.
pub fn parse_documents(text: &str, limits: Limits) -> Result<Vec<Document>, YamlError> {
    let mut sections: Vec<(Header, Vec<block::RawLine<'_>>)> = Vec::new();
    for (index, line) in strip_bom(text).lines().enumerate() {
        let number = index + 1;
        if let Some(rest) = line.strip_prefix("--- !u!") {
            if sections.len() == limits.max_documents {
                return Err(YamlError::Limit("document count"));
            }
            sections.push((header(number, rest)?, Vec::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push(block::RawLine { number, text: line });
        } else if !line.trim().is_empty() && !line.starts_with('%') {
            return Err(YamlError::syntax(
                number,
                "content before the first Unity document header",
            ));
        }
    }
    if sections.is_empty() {
        return Err(YamlError::syntax(1, "no Unity document headers"));
    }
    let mut ids = BTreeSet::new();
    sections
        .into_iter()
        .map(|(header, body)| {
            if !ids.insert(header.file_id) {
                return Err(YamlError::syntax(
                    header.line,
                    format!("duplicate fileID {}", header.file_id),
                ));
            }
            document(&header, &body, limits)
        })
        .collect()
}

/// Parses a header-less YAML mapping such as a `.meta` file or `ProjectVersion.txt`.
pub fn parse_plain(text: &str, limits: Limits) -> Result<Mapping, YamlError> {
    let lines: Vec<_> = strip_bom(text)
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.starts_with('%') && *line != "---")
        .map(|(index, text)| block::RawLine {
            number: index + 1,
            text,
        })
        .collect();
    match block::parse(&lines, limits)? {
        Node::Mapping(mapping) => Ok(mapping),
        _ => Err(YamlError::syntax(1, "expected a mapping")),
    }
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

fn header(line: usize, rest: &str) -> Result<Header, YamlError> {
    let invalid = || YamlError::syntax(line, format!("invalid document header `--- !u!{rest}`"));
    let mut parts = rest.split_whitespace();
    let class_id = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or_else(invalid)?;
    let file_id = parts
        .next()
        .and_then(|part| part.strip_prefix('&'))
        .and_then(|part| part.parse().ok())
        .ok_or_else(invalid)?;
    let stripped = match parts.next() {
        None => false,
        Some("stripped") => true,
        Some(_) => return Err(invalid()),
    };
    if parts.next().is_some() {
        return Err(invalid());
    }
    Ok(Header {
        class_id,
        file_id,
        stripped,
        line,
    })
}

fn document(
    header: &Header,
    body: &[block::RawLine<'_>],
    limits: Limits,
) -> Result<Document, YamlError> {
    let expected = || YamlError::syntax(header.line, "document must contain one type mapping");
    let Node::Mapping(mut root) = block::parse(body, limits)? else {
        return Err(expected());
    };
    if root.len() != 1 {
        return Err(expected());
    }
    let (type_name, value) = root.pop().ok_or_else(expected)?;
    let body = match value {
        Node::Mapping(body) => body,
        Node::Scalar(scalar) if scalar.is_empty() => Mapping::default(),
        _ => return Err(expected()),
    };
    Ok(Document {
        class_id: header.class_id,
        file_id: header.file_id,
        stripped: header.stripped,
        type_name,
        body,
    })
}
