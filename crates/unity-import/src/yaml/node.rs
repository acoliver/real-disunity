/// A parsed YAML value. Unity writes no tags, anchors or explicit nulls inside documents, so an
/// empty value is an empty plain scalar.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Scalar(Scalar),
    Sequence(Vec<Node>),
    Mapping(Mapping),
}

impl Node {
    pub fn as_scalar(&self) -> Option<&Scalar> {
        match self {
            Self::Scalar(scalar) => Some(scalar),
            _ => None,
        }
    }

    pub fn as_sequence(&self) -> Option<&[Node]> {
        match self {
            Self::Sequence(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_mapping(&self) -> Option<&Mapping> {
        match self {
            Self::Mapping(mapping) => Some(mapping),
            _ => None,
        }
    }

    /// Looks up `key` when this node is a mapping.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.as_mapping().and_then(|mapping| mapping.get(key))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarStyle {
    Plain,
    SingleQuoted,
    DoubleQuoted,
}

/// A scalar's decoded text and the style it was written in. Typed accessors only interpret
/// plain scalars, because a quoted `'1'` is a string in Unity's serializer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scalar {
    text: String,
    style: ScalarStyle,
}

impl Scalar {
    pub fn new(text: impl Into<String>, style: ScalarStyle) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, ScalarStyle::Plain)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn style(&self) -> ScalarStyle {
        self.style
    }

    /// An empty plain scalar, as written for `key:` with no value.
    pub fn is_empty(&self) -> bool {
        self.style == ScalarStyle::Plain && self.text.is_empty()
    }

    pub fn as_i64(&self) -> Option<i64> {
        self.plain_text()?.parse().ok()
    }

    pub fn as_f64(&self) -> Option<f64> {
        self.plain_text()?.parse().ok()
    }

    /// Unity serializes booleans as `0` and `1`; YAML spellings are also accepted.
    pub fn as_bool(&self) -> Option<bool> {
        match self.plain_text()? {
            "0" | "false" => Some(false),
            "1" | "true" => Some(true),
            _ => None,
        }
    }

    fn plain_text(&self) -> Option<&str> {
        (self.style == ScalarStyle::Plain).then_some(self.text.as_str())
    }
}

/// A mapping in source order. Unity 2017 serializes map fields as repeated `data` keys, so keys
/// may repeat; [`Mapping::get`] returns the first entry and [`Mapping::get_all`] every entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mapping {
    entries: Vec<(String, Node)>,
}

impl Mapping {
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.entries
            .iter()
            .find_map(|(name, value)| (name == key).then_some(value))
    }

    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Node> {
        self.entries
            .iter()
            .filter(move |(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Node)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn push(&mut self, key: &str, value: Node) {
        self.entries.push((key.to_owned(), value));
    }

    pub(crate) fn pop(&mut self) -> Option<(String, Node)> {
        self.entries.pop()
    }
}
