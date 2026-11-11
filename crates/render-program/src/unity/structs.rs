//! Where the struct of a player's constant buffer sits in the buffer.
//!
//! Measured on the Old World player, in 5,605 bound buffers with structs (see the README): each
//! has exactly one struct and no members of its own, the struct's `index` is 0 and it is the
//! only member of the SPIR-V block, at `Offset` 0. Its array size is the length of the block's
//! array and its size is the array's `ArrayStride`. The offsets of its members are from the
//! start of an element: every member ends within the struct's size, which absolute offsets could
//! not do for a second element. This module places exactly that shape and refuses every other.

use super::buffers::{BufferLayout, ParamName, StructArray};
use super::constants::ConstantError;

/// Where a buffer's struct array starts and how its elements are spaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructPlacement {
    /// Byte offset of the first element.
    pub start: usize,
    /// How many elements there are; at least 1.
    pub count: usize,
    /// Bytes from one element to the next.
    pub stride: usize,
}

impl BufferLayout {
    /// The placement of the buffer's struct, or `None` for a buffer without one.
    ///
    /// # Errors
    /// [`ConstantError::UnsupportedStruct`] for an editor-style struct, a struct beside members
    /// or another struct, and a struct that does not start at byte 0: none of those has been
    /// measured. [`ConstantError::NegativeSize`] for a negative array size or struct size.
    pub fn struct_placement(
        &self,
        names: &[(String, i32)],
    ) -> Result<Option<(&StructArray, StructPlacement)>, ConstantError> {
        let unsupported = |name: &ParamName| {
            name.resolve(names)
                .map(|name| ConstantError::UnsupportedStruct {
                    name: name.to_owned(),
                })
        };
        if let Some(name) = self.editor_structs.first() {
            return Err(unsupported(name)?);
        }
        let Some(first) = self.structs.first() else {
            return Ok(None);
        };
        let refused = if !self.members.is_empty() {
            Some(first)
        } else if self.structs.len() > 1 {
            self.structs.get(1)
        } else if first.index != 0 {
            Some(first)
        } else {
            None
        };
        if let Some(refused) = refused {
            return Err(unsupported(&refused.name)?);
        }
        let negative = |field, value| ConstantError::NegativeSize {
            name: first.name.resolve(names).unwrap_or("?").to_owned(),
            field,
            value,
        };
        let count = usize::try_from(first.array_size)
            .map_err(|_| negative("struct array size", first.array_size))?;
        let stride =
            usize::try_from(first.size).map_err(|_| negative("struct size", first.size))?;
        Ok(Some((
            first,
            StructPlacement {
                start: 0,
                count: count.max(1),
                stride,
            },
        )))
    }
}
