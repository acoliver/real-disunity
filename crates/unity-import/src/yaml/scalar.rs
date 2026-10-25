use super::{Scalar, ScalarStyle, YamlError};

/// Whether `text`, starting with a quote, contains that quote's closing quote.
pub(crate) fn quoted_complete(text: &str) -> bool {
    closing(text).is_some()
}

/// A complete quoted scalar with nothing but whitespace after the closing quote.
pub(crate) fn quoted(text: &str, line: usize) -> Result<Scalar, YamlError> {
    let (scalar, end) = read_quoted(text, line)?;
    if !text[end..].trim().is_empty() {
        return Err(YamlError::syntax(line, "content after a quoted scalar"));
    }
    Ok(scalar)
}

/// Decodes the quoted scalar at the start of `text` and returns the byte offset after it.
pub(crate) fn read_quoted(text: &str, line: usize) -> Result<(Scalar, usize), YamlError> {
    let close = closing(text).ok_or_else(|| YamlError::syntax(line, "unterminated quote"))?;
    let body = &text[1..close];
    let scalar = if text.starts_with('\'') {
        Scalar::new(
            fold(body, false).replace("''", "'"),
            ScalarStyle::SingleQuoted,
        )
    } else {
        Scalar::new(
            unescape(&fold(body, true), line)?,
            ScalarStyle::DoubleQuoted,
        )
    };
    Ok((scalar, close + 1))
}

/// Byte offset of the quote closing the quoted scalar at the start of `text`.
fn closing(text: &str) -> Option<usize> {
    let quote = text.chars().next()?;
    let mut chars = text.char_indices().skip(1).peekable();
    while let Some((index, c)) = chars.next() {
        match (quote, c) {
            ('\'', '\'') if chars.peek().is_some_and(|(_, next)| *next == '\'') => {
                chars.next();
            }
            ('"', '\\') => {
                chars.next();
            }
            (q, c) if q == c => return Some(index),
            _ => {}
        }
    }
    None
}

/// YAML line folding: a single line break becomes a space and each empty line a newline.
/// In double-quoted scalars a line ending in an escaping backslash joins without a space.
fn fold(body: &str, escapes: bool) -> String {
    let lines: Vec<&str> = body.split('\n').collect();
    let mut text = String::new();
    let mut blanks = 0;
    let mut join = false;
    for (index, line) in lines.iter().enumerate() {
        let mut part = if index == 0 { *line } else { line.trim_start() };
        if index + 1 < lines.len() {
            part = part.trim_end();
        }
        if index > 0 && part.is_empty() && index + 1 < lines.len() {
            blanks += 1;
            continue;
        }
        if index > 0 && !join {
            if blanks == 0 {
                text.push(' ');
            } else {
                text.extend(std::iter::repeat_n('\n', blanks));
            }
        }
        blanks = 0;
        join = escapes && index + 1 < lines.len() && ends_with_escape(part);
        text.push_str(if join { &part[..part.len() - 1] } else { part });
    }
    text
}

fn ends_with_escape(part: &str) -> bool {
    part.bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
}

fn unescape(text: &str, line: usize) -> Result<String, YamlError> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let escape = chars
            .next()
            .ok_or_else(|| YamlError::syntax(line, "dangling escape"))?;
        out.push(match escape {
            '0' => '\0',
            'a' => '\u{7}',
            'b' => '\u{8}',
            't' | '\t' => '\t',
            'n' => '\n',
            'v' => '\u{b}',
            'f' => '\u{c}',
            'r' => '\r',
            'e' => '\u{1b}',
            ' ' => ' ',
            '"' => '"',
            '/' => '/',
            '\\' => '\\',
            'N' => '\u{85}',
            '_' => '\u{a0}',
            'L' => '\u{2028}',
            'P' => '\u{2029}',
            'x' => hex(&mut chars, 2, line)?,
            'u' => hex(&mut chars, 4, line)?,
            'U' => hex(&mut chars, 8, line)?,
            other => {
                return Err(YamlError::syntax(
                    line,
                    format!("unknown escape `\\{other}`"),
                ));
            }
        });
    }
    Ok(out)
}

fn hex(chars: &mut std::str::Chars<'_>, digits: usize, line: usize) -> Result<char, YamlError> {
    let code: String = chars.by_ref().take(digits).collect();
    if code.len() != digits {
        return Err(YamlError::syntax(line, "short hexadecimal escape"));
    }
    u32::from_str_radix(&code, 16)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| YamlError::syntax(line, format!("invalid escape code `{code}`")))
}
