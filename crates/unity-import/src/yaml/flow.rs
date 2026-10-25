use super::{Mapping, Node, Scalar, YamlError, scalar};

/// Whether every flow bracket opened in `text` is closed, ignoring quoted text.
pub(crate) fn complete(text: &str) -> bool {
    let mut depth = 0_usize;
    let mut quote = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(open) = quote {
            if escapes_next(open, c, chars.peek()) {
                chars.next();
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '{' | '[' => depth += 1,
            '}' | ']' => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return true,
            },
            _ => {}
        }
    }
    depth == 0 && quote.is_none()
}

/// Inside quotes, `\` escapes the next character in double quotes and `''` is a literal quote
/// in single quotes.
fn escapes_next(quote: char, c: char, next: Option<&char>) -> bool {
    (quote == '"' && c == '\\') || (quote == '\'' && c == '\'' && next == Some(&'\''))
}

/// Parses one complete flow collection.
pub(crate) fn parse(text: &str, line: usize, depth: usize) -> Result<Node, YamlError> {
    let mut cursor = Cursor {
        text,
        pos: 0,
        line,
        depth,
    };
    let node = cursor.value()?;
    cursor.skip_space();
    if cursor.pos != text.len() {
        return Err(cursor.error("content after a flow collection"));
    }
    Ok(node)
}

struct Cursor<'a> {
    text: &'a str,
    pos: usize,
    line: usize,
    depth: usize,
}

impl Cursor<'_> {
    fn error(&self, message: &str) -> YamlError {
        YamlError::syntax(self.line, message)
    }

    fn rest(&self) -> &str {
        &self.text[self.pos..]
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip_space();
        let found = self.rest().starts_with(c);
        if found {
            self.pos += c.len_utf8();
        }
        found
    }

    fn value(&mut self) -> Result<Node, YamlError> {
        self.skip_space();
        if self.eat('{') {
            return self.nested(Self::mapping);
        }
        if self.eat('[') {
            return self.nested(Self::sequence);
        }
        self.scalar(&[',', '}', ']']).map(Node::Scalar)
    }

    fn nested(
        &mut self,
        parse: fn(&mut Self) -> Result<Node, YamlError>,
    ) -> Result<Node, YamlError> {
        self.depth = self
            .depth
            .checked_sub(1)
            .ok_or(YamlError::Limit("nesting depth"))?;
        let node = parse(self);
        self.depth += 1;
        node
    }

    fn mapping(&mut self) -> Result<Node, YamlError> {
        let mut mapping = Mapping::default();
        if self.eat('}') {
            return Ok(Node::Mapping(mapping));
        }
        loop {
            let key = self.scalar(&[':', ',', '}'])?;
            if !self.eat(':') {
                return Err(self.error("expected `:` in a flow mapping"));
            }
            let value = self.value()?;
            mapping.push(key.text(), value);
            if self.eat('}') {
                return Ok(Node::Mapping(mapping));
            }
            if !self.eat(',') {
                return Err(self.error("expected `,` or `}` in a flow mapping"));
            }
        }
    }

    fn sequence(&mut self) -> Result<Node, YamlError> {
        let mut items = Vec::new();
        if self.eat(']') {
            return Ok(Node::Sequence(items));
        }
        loop {
            items.push(self.value()?);
            if self.eat(']') {
                return Ok(Node::Sequence(items));
            }
            if !self.eat(',') {
                return Err(self.error("expected `,` or `]` in a flow sequence"));
            }
        }
    }

    /// A quoted scalar, or plain text up to one of `stops`. Line breaks fold to spaces.
    fn scalar(&mut self, stops: &[char]) -> Result<Scalar, YamlError> {
        self.skip_space();
        if self.rest().starts_with(['\'', '"']) {
            let (scalar, length) = scalar::read_quoted(self.rest(), self.line)?;
            self.pos += length;
            return Ok(scalar);
        }
        let length = self.rest().find(stops).unwrap_or(self.rest().len());
        let text = self.rest()[..length]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        self.pos += length;
        Ok(Scalar::plain(text))
    }
}
