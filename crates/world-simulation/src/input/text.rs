/// Committed Unicode from the platform, separate from key identities and key edges.
/// Preedit cursor offsets are UTF-8 byte offsets, as supplied by the IME.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInput {
    Enabled,
    Preedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    Commit(String),
    Cancel,
    Disabled,
}
impl TextInput {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Commit(text) => text.len() <= 65536,
            Self::Preedit { text, cursor } => {
                text.len() <= 65536
                    && cursor
                        .is_none_or(|(a, b)| text.is_char_boundary(a) && text.is_char_boundary(b))
            }
            _ => true,
        }
    }
}
