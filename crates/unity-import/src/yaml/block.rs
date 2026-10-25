use super::{Limits, Mapping, Node, Scalar, YamlError, flow, scalar};

pub(crate) struct RawLine<'a> {
    pub number: usize,
    pub text: &'a str,
}

/// A source line split into indentation and content. Blank lines have empty text.
#[derive(Clone, Copy)]
struct Line<'a> {
    number: usize,
    indent: usize,
    text: &'a str,
}

impl<'a> Line<'a> {
    fn new(raw: &RawLine<'a>) -> Result<Self, YamlError> {
        let text = raw.text.trim_end();
        let content = text.trim_start_matches(' ');
        if content.starts_with('\t') {
            return Err(YamlError::syntax(raw.number, "tab indentation"));
        }
        Ok(Self {
            number: raw.number,
            indent: text.len() - content.len(),
            text: content,
        })
    }
}

pub(crate) fn parse(raw: &[RawLine<'_>], limits: Limits) -> Result<Node, YamlError> {
    let lines = raw.iter().map(Line::new).collect::<Result<Vec<_>, _>>()?;
    let mut parser = Parser {
        lines,
        pos: 0,
        depth: 0,
        max_depth: limits.max_depth,
    };
    let Some(first) = parser.next_content() else {
        return Ok(Node::Mapping(Mapping::default()));
    };
    let node = parser.node(first.indent)?;
    match parser.next_content() {
        Some(line) => Err(YamlError::syntax(line.number, "unexpected indentation")),
        None => Ok(node),
    }
}

struct Parser<'a> {
    lines: Vec<Line<'a>>,
    pos: usize,
    depth: usize,
    max_depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Line<'a>> {
        self.lines.get(self.pos).copied()
    }

    fn next_content(&mut self) -> Option<Line<'a>> {
        while self.peek().is_some_and(|line| line.text.is_empty()) {
            self.pos += 1;
        }
        self.peek()
    }

    /// A block node whose first line is the current line, at `indent`.
    fn node(&mut self, indent: usize) -> Result<Node, YamlError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(YamlError::Limit("nesting depth"));
        }
        let line = self.peek().ok_or(YamlError::Limit("input"))?;
        let node = if is_item(line.text) {
            self.sequence(indent).map(Node::Sequence)
        } else {
            self.mapping(indent).map(Node::Mapping)
        };
        self.depth -= 1;
        node
    }

    fn mapping(&mut self, indent: usize) -> Result<Mapping, YamlError> {
        let mut mapping = Mapping::default();
        while let Some(line) = self.next_content() {
            if line.indent < indent {
                break;
            }
            if line.indent > indent || is_item(line.text) {
                return Err(YamlError::syntax(line.number, "unexpected indentation"));
            }
            let (key, rest) = split_key(line.text)
                .ok_or_else(|| YamlError::syntax(line.number, "expected `key: value`"))?;
            self.pos += 1;
            let value = self.value(rest, indent, line.number)?;
            mapping.push(key, value);
        }
        Ok(mapping)
    }

    fn sequence(&mut self, indent: usize) -> Result<Vec<Node>, YamlError> {
        let mut items = Vec::new();
        while let Some(line) = self.next_content() {
            if line.indent < indent || (line.indent == indent && !is_item(line.text)) {
                break;
            }
            if line.indent > indent {
                return Err(YamlError::syntax(line.number, "unexpected indentation"));
            }
            items.push(self.item(line)?);
        }
        Ok(items)
    }

    fn item(&mut self, line: Line<'a>) -> Result<Node, YamlError> {
        let content = line.text[1..].trim_start();
        if is_item(content) || split_key(content).is_some() {
            // The item's content is a block node starting at the column after `- `.
            let column = line.indent + line.text.len() - content.len();
            self.lines[self.pos] = Line {
                number: line.number,
                indent: column,
                text: content,
            };
            return self.node(column);
        }
        self.pos += 1;
        if !content.is_empty() {
            return self.value(content, line.indent, line.number);
        }
        match self.next_content() {
            Some(next) if next.indent > line.indent => self.node(next.indent),
            _ => Ok(Node::Scalar(Scalar::plain(""))),
        }
    }

    /// The value following `key:` or `- ` on a line owned by a node at `indent`.
    fn value(&mut self, rest: &str, indent: usize, number: usize) -> Result<Node, YamlError> {
        match rest.chars().next() {
            None => self.nested(indent),
            Some('{' | '[') => {
                let text = self.continued(rest, indent, number, flow::complete)?;
                flow::parse(&text, number, self.max_depth.saturating_sub(self.depth))
            }
            Some('\'' | '"') => {
                let text = self.continued(rest, indent, number, scalar::quoted_complete)?;
                scalar::quoted(&text, number).map(Node::Scalar)
            }
            Some(_) => Ok(Node::Scalar(Scalar::plain(self.plain(rest, indent)))),
        }
    }

    /// The block value of `key:`. Unity writes a sequence under a key at the key's indentation.
    fn nested(&mut self, indent: usize) -> Result<Node, YamlError> {
        match self.next_content() {
            Some(line) if line.indent > indent => self.node(line.indent),
            Some(line) if line.indent == indent && is_item(line.text) => self.node(indent),
            _ => Ok(Node::Scalar(Scalar::plain(""))),
        }
    }

    /// Joins continuation lines, more indented than the owner, until `complete` accepts.
    fn continued(
        &mut self,
        first: &str,
        indent: usize,
        number: usize,
        complete: fn(&str) -> bool,
    ) -> Result<String, YamlError> {
        let mut text = first.to_owned();
        while !complete(&text) {
            let line = self
                .peek()
                .filter(|line| line.text.is_empty() || line.indent > indent)
                .ok_or_else(|| YamlError::syntax(number, "unterminated value"))?;
            self.pos += 1;
            text.push('\n');
            text.push_str(line.text);
        }
        Ok(text)
    }

    /// A plain scalar with folded continuation lines.
    fn plain(&mut self, first: &str, indent: usize) -> String {
        let mut text = first.to_owned();
        loop {
            let start = self.pos;
            let Some(line) = self.next_content() else {
                break;
            };
            if line.indent <= indent {
                break;
            }
            let blanks = self.pos - start;
            if blanks == 0 {
                text.push(' ');
            } else {
                text.extend(std::iter::repeat_n('\n', blanks));
            }
            text.push_str(line.text);
            self.pos += 1;
        }
        text
    }
}

fn is_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

/// Splits `key: value` or `key:`. Keys are plain; flow and quoted text is never a key.
fn split_key(text: &str) -> Option<(&str, &str)> {
    if text.starts_with(['{', '[', '\'', '"']) {
        return None;
    }
    let end = text
        .find(": ")
        .or_else(|| text.strip_suffix(':').map(str::len))?;
    let key = text[..end].trim_end();
    (!key.is_empty()).then(|| (key, text[end + 1..].trim()))
}
