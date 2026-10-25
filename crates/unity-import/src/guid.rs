//! Unity asset GUIDs and serialized object references.

use std::{fmt, str::FromStr};

use crate::yaml::{Node, Scalar};

/// A 128-bit Unity asset GUID, written as 32 lowercase hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Guid(u128);

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({self})")
    }
}

impl Guid {
    pub const ZERO: Self = Self(0);

    /// Unity's built-in resources (`0000000000000000e000000000000000`) and default resources
    /// (`0000000000000000f000000000000000`), which have no file in the project.
    pub fn is_builtin(self) -> bool {
        matches!(self.0, 0xe000_0000_0000_0000 | 0xf000_0000_0000_0000)
    }
}

impl FromStr for Guid {
    type Err = ReferenceError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ReferenceError::Guid(text.to_owned()));
        }
        u128::from_str_radix(text, 16)
            .map(Self)
            .map_err(|_| ReferenceError::Guid(text.to_owned()))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReferenceError {
    #[error("invalid asset GUID `{0}`")]
    Guid(String),
    #[error("object reference must be a mapping with an integer fileID")]
    Shape,
}

/// A serialized `{fileID, guid, type}` reference. A reference without a GUID names an object in
/// the same file; `fileID: 0` without a GUID is the null reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectRef {
    pub file_id: i64,
    pub guid: Option<Guid>,
    pub type_id: Option<i32>,
}

impl ObjectRef {
    pub fn from_node(node: &Node) -> Result<Self, ReferenceError> {
        let mapping = node.as_mapping().ok_or(ReferenceError::Shape)?;
        let file_id = mapping
            .get("fileID")
            .and_then(Node::as_scalar)
            .and_then(Scalar::as_i64)
            .ok_or(ReferenceError::Shape)?;
        let guid = match mapping.get("guid").and_then(Node::as_scalar) {
            Some(scalar) => Some(scalar.text().parse::<Guid>()?).filter(|g| *g != Guid::ZERO),
            None => None,
        };
        let type_id = mapping
            .get("type")
            .and_then(Node::as_scalar)
            .and_then(Scalar::as_i64)
            .and_then(|value| i32::try_from(value).ok());
        Ok(Self {
            file_id,
            guid,
            type_id,
        })
    }

    pub fn is_null(&self) -> bool {
        self.file_id == 0 && self.guid.is_none()
    }

    pub fn is_local(&self) -> bool {
        self.guid.is_none()
    }
}
