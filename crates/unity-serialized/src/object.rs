//! Decoding one object's data with a class layout.
//!
//! Player builds strip type trees, so each class has a layout written here from its observed
//! data. Layouts match Unity 2019.1 through Unity 6. An object decodes only if its layout
//! consumes exactly its bytes, so a layout that drifts from the data fails instead of
//! returning wrong values. Offsets in errors are bytes from the start of the object.

use crate::{ClassId, ObjectInfo, SerializedError, cursor::Cursor};

/// A reference to an object: `file_id` 0 is the same file, `n > 0` names externals `n - 1`.
/// `path_id` 0 is a null reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PPtr {
    pub file_id: i32,
    pub path_id: i64,
}

impl PPtr {
    #[must_use]
    pub fn is_null(self) -> bool {
        self.path_id == 0
    }
}

/// Bytes of a [`PPtr`]: a 32-bit file ID and a 64-bit path ID.
const PPTR_LEN: usize = 12;

/// A class layout: reads the class's fields in their serialized order.
pub trait Layout: Sized {
    const CLASS: ClassId;

    /// Reads the fields. [`decode`] checks the class and that no bytes are left over.
    ///
    /// # Errors
    /// Fails when the data is truncated or malformed for this layout.
    fn read(reader: &mut ObjectReader<'_>) -> Result<Self, SerializedError>;
}

/// Decodes an object with its class's layout. `bytes` are the object's own bytes, as
/// [`object_bytes`] returns them from a whole file; `big_endian` is the file header's flag.
///
/// # Errors
/// Fails when the object is of another class, `bytes` is not the object's size, the data is
/// truncated or malformed for the layout, or the layout leaves bytes unread.
pub fn decode<T: Layout>(
    bytes: &[u8],
    object: &ObjectInfo,
    big_endian: bool,
) -> Result<T, SerializedError> {
    if object.class_id != T::CLASS {
        return Err(SerializedError::WrongClass {
            path_id: object.path_id,
            expected: T::CLASS,
            found: object.class_id,
        });
    }
    if bytes.len() != object.size as usize {
        return Err(SerializedError::Truncated {
            offset: 0,
            needed: object.size as usize,
            len: bytes.len(),
        });
    }
    let mut reader = ObjectReader::new(bytes, big_endian);
    let value = T::read(&mut reader)?;
    reader.finish(object)?;
    Ok(value)
}

/// The byte range of an object within `data`.
///
/// # Errors
/// Fails when the range lies outside `data`.
pub fn object_bytes<'a>(data: &'a [u8], object: &ObjectInfo) -> Result<&'a [u8], SerializedError> {
    let start = usize::try_from(object.offset).ok();
    let end = start.and_then(|s| s.checked_add(object.size as usize));
    start
        .zip(end)
        .and_then(|(start, end)| data.get(start..end))
        .ok_or(SerializedError::Truncated {
            offset: start.unwrap_or(usize::MAX),
            needed: object.size as usize,
            len: data.len(),
        })
}

/// Reads one object's fields. Alignment is relative to the start of the object.
pub struct ObjectReader<'a> {
    cursor: Cursor<'a>,
}

impl<'a> ObjectReader<'a> {
    #[must_use]
    pub fn new(bytes: &'a [u8], big_endian: bool) -> Self {
        Self {
            cursor: Cursor::new(bytes, 0, big_endian),
        }
    }

    /// Bytes read so far.
    #[must_use]
    pub fn position(&self) -> usize {
        self.cursor.position()
    }

    /// Bytes not yet read. Script data follows the `MonoBehaviour` header here.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.cursor.remaining()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn i32(&mut self) -> Result<i32, SerializedError> {
        self.cursor.i32()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn u32(&mut self) -> Result<u32, SerializedError> {
        self.cursor.u32()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn u16(&mut self) -> Result<u16, SerializedError> {
        self.cursor.u16()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn u8(&mut self) -> Result<u8, SerializedError> {
        self.cursor.u8()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn f32(&mut self) -> Result<f32, SerializedError> {
        self.cursor.f32()
    }

    /// # Errors
    /// Fails when the data ends early or the byte is neither 0 nor 1.
    pub fn bool(&mut self) -> Result<bool, SerializedError> {
        self.cursor.bool()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn bytes<const N: usize>(&mut self) -> Result<[u8; N], SerializedError> {
        self.cursor.array()
    }

    /// Skips padding to the next multiple of four bytes.
    ///
    /// # Errors
    /// Fails when the data ends early.
    pub fn align(&mut self) -> Result<(), SerializedError> {
        self.cursor.align4()
    }

    /// A length-prefixed UTF-8 string, padded to four bytes.
    ///
    /// # Errors
    /// Fails when the data ends early, the length does not fit, or the text is not UTF-8.
    pub fn string(&mut self) -> Result<String, SerializedError> {
        self.cursor.sized_string()
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn pptr(&mut self) -> Result<PPtr, SerializedError> {
        Ok(PPtr {
            file_id: self.cursor.i32()?,
            path_id: self.cursor.i64()?,
        })
    }

    /// # Errors
    /// Fails when the data ends early.
    pub fn floats<const N: usize>(&mut self) -> Result<[f32; N], SerializedError> {
        let mut out = [0.0; N];
        for value in &mut out {
            *value = self.f32()?;
        }
        Ok(out)
    }

    /// A length-prefixed array of references, padded to four bytes.
    ///
    /// # Errors
    /// Fails when the data ends early or the count does not fit.
    pub fn pptrs(&mut self, what: &'static str) -> Result<Vec<PPtr>, SerializedError> {
        let count = self.cursor.count(what, PPTR_LEN)?;
        let items = (0..count).map(|_| self.pptr()).collect();
        self.align()?;
        items
    }

    /// Consumes the rest of the object and returns its range, for data a later stage decodes.
    pub fn rest(&mut self) -> std::ops::Range<usize> {
        let start = self.position();
        let len = self.remaining();
        // `take` cannot fail for exactly the remaining length.
        self.cursor
            .take(len)
            .map_or(start..start, |_| start..start + len)
    }

    fn finish(&self, object: &ObjectInfo) -> Result<(), SerializedError> {
        if self.remaining() == 0 {
            return Ok(());
        }
        Err(SerializedError::TrailingBytes {
            path_id: object.path_id,
            class: object.class_id,
            read: self.position(),
            size: object.size,
        })
    }
}
