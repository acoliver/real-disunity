//! Encoder diagnostics share the timer's typed identities, independently of query support.
use std::{borrow::Cow, fmt};

use crate::{PassEntry, PassId, StageId};

impl PassId {
    /// The spelling shared by encoder diagnostics and timing reports. Authored labels
    /// with levels or occurrences allocate and must be retained when building a schedule.
    /// Geometry uses a bounded stack buffer: `Geometry(role)` for a role's first entry and
    /// `Geometry(role#n)` for the nth entry after it.
    pub fn encoder_label(self) -> impl AsRef<str> {
        match self {
            Self::Geometry(entry) => EncoderLabel::geometry(entry),
            Self::Resolve => EncoderLabel::Static("Resolve"),
            Self::Stage(stage) => EncoderLabel::Authored(stage.diagnostic_label()),
            Self::Operation(_) => EncoderLabel::Authored(Cow::Owned(format!("{self:?}"))),
        }
    }
}

impl fmt::Display for PassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.encoder_label().as_ref())
    }
}

impl StageId {
    pub(crate) fn diagnostic_label(self) -> Cow<'static, str> {
        if self.level == 0 {
            Cow::Borrowed(self.name)
        } else {
            Cow::Owned(format!("{} level {}", self.name, self.level))
        }
    }
}

enum EncoderLabel {
    Static(&'static str),
    Authored(Cow<'static, str>),
    Geometry { bytes: [u8; 64], length: usize },
}

impl EncoderLabel {
    fn geometry(entry: PassEntry) -> Self {
        use std::io::Write;
        // The longest role name with a u32 index, `#`, a u32 occurrence and delimiters is
        // 55 bytes, which fits in 64.
        let mut bytes = [0; 64];
        let mut remaining = bytes.as_mut_slice();
        write!(remaining, "Geometry({entry})").expect("bounded geometry label");
        let unused = remaining.len();
        let length = bytes.len() - unused;
        Self::Geometry { bytes, length }
    }
}

impl AsRef<str> for EncoderLabel {
    fn as_ref(&self) -> &str {
        match self {
            Self::Static(name) => name,
            Self::Authored(name) => name.as_ref(),
            Self::Geometry { bytes, length } => {
                std::str::from_utf8(&bytes[..*length]).expect("ASCII geometry label")
            }
        }
    }
}
