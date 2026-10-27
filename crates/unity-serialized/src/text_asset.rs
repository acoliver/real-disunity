//! The `TextAsset` layout.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader},
};

/// A text or binary file shipped with the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextAsset {
    pub name: String,
    /// Byte range of the contents within the object. The contents need not be UTF-8.
    pub contents: Range<usize>,
}

impl Layout for TextAsset {
    const CLASS: ClassId = ClassId::TEXT_ASSET;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            contents: r.byte_range("text asset byte")?,
        })
    }
}
