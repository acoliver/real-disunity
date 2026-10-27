//! Bounds-checked reads over a byte slice. Positions are absolute offsets into the file, so
//! alignment matches Unity's, which aligns relative to the start of the file.

use crate::SerializedError;

pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    big_endian: bool,
}

macro_rules! numbers {
    ($($name:ident: $ty:ty),* $(,)?) => {
        $(
            pub(crate) fn $name(&mut self) -> Result<$ty, SerializedError> {
                let bytes = self.array()?;
                Ok(if self.big_endian {
                    <$ty>::from_be_bytes(bytes)
                } else {
                    <$ty>::from_le_bytes(bytes)
                })
            }
        )*
    };
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize, big_endian: bool) -> Self {
        Self {
            data,
            pos,
            big_endian,
        }
    }

    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub(crate) fn take(&mut self, needed: usize) -> Result<&'a [u8], SerializedError> {
        let truncated = SerializedError::Truncated {
            offset: self.pos,
            needed,
            len: self.data.len(),
        };
        let end = self.pos.checked_add(needed).ok_or(truncated.clone())?;
        let bytes = self.data.get(self.pos..end).ok_or(truncated)?;
        self.pos = end;
        Ok(bytes)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], SerializedError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    numbers!(u16: u16, i16: i16, u32: u32, i32: i32, u64: u64, i64: i64);

    pub(crate) fn u8(&mut self) -> Result<u8, SerializedError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn bool(&mut self) -> Result<bool, SerializedError> {
        let offset = self.pos;
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(SerializedError::InvalidBool { offset, value }),
        }
    }

    /// Skips to the next multiple of four bytes from the start of the file.
    pub(crate) fn align4(&mut self) -> Result<(), SerializedError> {
        let pad = (4 - self.pos % 4) % 4;
        self.take(pad).map(|_| ())
    }

    /// Reads a zero-terminated UTF-8 string and the terminator.
    pub(crate) fn string(&mut self) -> Result<String, SerializedError> {
        let offset = self.pos;
        let rest = self.data.get(offset..).unwrap_or_default();
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(SerializedError::UnterminatedString { offset })?;
        let text = std::str::from_utf8(&rest[..len])
            .map_err(|_| SerializedError::InvalidString { offset })?
            .to_owned();
        self.pos += len + 1;
        Ok(text)
    }

    /// Reads an entry count, rejecting one whose entries cannot fit in the remaining bytes.
    pub(crate) fn count(
        &mut self,
        what: &'static str,
        min_entry: usize,
    ) -> Result<usize, SerializedError> {
        let offset = self.pos;
        let count = self.i32()?;
        let remaining = self.remaining();
        usize::try_from(count)
            .ok()
            .filter(|&n| {
                n.checked_mul(min_entry)
                    .is_some_and(|size| size <= remaining)
            })
            .ok_or(SerializedError::InvalidCount {
                what,
                count,
                offset,
                remaining,
            })
    }
}
