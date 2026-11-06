//! The byte-level reader and writer, and the `Wire` trait that the snapshot's types implement.
//!
//! Every integer is little-endian and every float is its IEEE bit pattern, so a value round-trips
//! exactly. A string, a byte run, a list or a map starts with its element count as a `u32`, which
//! the reader checks against a fixed cap and against the bytes that are left before it allocates.
use std::collections::BTreeMap;

use super::WireError;

/// Most elements one list or map holds.
pub const MAX_ITEMS: u32 = 1 << 22;
/// Most bytes one string holds.
pub const MAX_TEXT_BYTES: u32 = 1 << 16;

pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub const fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Writes an element count, which is at most `limit`.
    pub fn count(&mut self, count: usize, limit: u32) -> Result<(), WireError> {
        match u32::try_from(count) {
            Ok(count) if count <= limit => {
                self.raw(&count.to_le_bytes());
                Ok(())
            }
            _ => Err(WireError::TooLong),
        }
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub const fn remaining(&self) -> usize {
        self.bytes.len()
    }

    pub fn take(&mut self, count: usize) -> Result<&'a [u8], WireError> {
        let (head, tail) = self
            .bytes
            .split_at_checked(count)
            .ok_or(WireError::Truncated)?;
        self.bytes = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        <[u8; N]>::try_from(self.take(N)?).map_err(|_| WireError::Truncated)
    }

    /// Reads an element count of at most `limit`, each element needing at least one byte.
    pub fn count(&mut self, limit: u32) -> Result<usize, WireError> {
        let count = u32::from_le_bytes(self.array()?);
        if count > limit {
            return Err(WireError::TooLong);
        }
        let count = usize::try_from(count).map_err(|_| WireError::TooLong)?;
        if count > self.remaining() {
            return Err(WireError::Truncated);
        }
        Ok(count)
    }
}

/// A type with a fixed byte encoding.
pub trait Wire: Sized {
    fn write(&self, out: &mut Writer) -> Result<(), WireError>;
    fn read(input: &mut Reader<'_>) -> Result<Self, WireError>;
}

macro_rules! wire_number {
    ($($ty:ty),*) => {$(
        impl Wire for $ty {
            fn write(&self, out: &mut Writer) -> Result<(), WireError> {
                out.raw(&self.to_le_bytes());
                Ok(())
            }
            fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
                Ok(Self::from_le_bytes(input.array()?))
            }
        }
    )*};
}

wire_number!(u8, u32, u64, i32, i64, f32, f64);

impl Wire for bool {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        u8::from(*self).write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(WireError::UnknownTag(u32::from(other))),
        }
    }
}

impl Wire for String {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        out.count(self.len(), MAX_TEXT_BYTES)?;
        out.raw(self.as_bytes());
        Ok(())
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let length = input.count(MAX_TEXT_BYTES)?;
        String::from_utf8(input.take(length)?.to_vec()).map_err(|_| WireError::BadText)
    }
}

impl<T: Wire + Copy + Default, const N: usize> Wire for [T; N] {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        self.iter().try_for_each(|item| item.write(out))
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let mut items = [T::default(); N];
        for item in &mut items {
            *item = T::read(input)?;
        }
        Ok(items)
    }
}

impl<T: Wire> Wire for Option<T> {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            None => 0_u8.write(out),
            Some(value) => {
                1_u8.write(out)?;
                value.write(out)
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            0 => Ok(None),
            1 => Ok(Some(T::read(input)?)),
            other => Err(WireError::UnknownTag(u32::from(other))),
        }
    }
}

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        self.0.write(out)?;
        self.1.write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok((A::read(input)?, B::read(input)?))
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        out.count(self.len(), MAX_ITEMS)?;
        self.iter().try_for_each(|item| item.write(out))
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let count = input.count(MAX_ITEMS)?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            items.push(T::read(input)?);
        }
        Ok(items)
    }
}

impl<K: Wire + Ord, V: Wire> Wire for BTreeMap<K, V> {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        out.count(self.len(), MAX_ITEMS)?;
        for (key, value) in self {
            key.write(out)?;
            value.write(out)?;
        }
        Ok(())
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let count = input.count(MAX_ITEMS)?;
        let mut map = Self::new();
        for _ in 0..count {
            let key = K::read(input)?;
            if map.insert(key, V::read(input)?).is_some() {
                return Err(WireError::DuplicateKey);
            }
        }
        Ok(map)
    }
}

/// A byte run: a `u32` length, then the bytes. `limit` is the most bytes it may hold.
pub fn write_bytes(out: &mut Writer, bytes: &[u8], limit: u32) -> Result<(), WireError> {
    out.count(bytes.len(), limit)?;
    out.raw(bytes);
    Ok(())
}

/// Reads a byte run written by [`write_bytes`] with the same `limit`.
pub fn read_bytes(input: &mut Reader<'_>, limit: u32) -> Result<Vec<u8>, WireError> {
    let length = input.count(limit)?;
    Ok(input.take(length)?.to_vec())
}

/// Implements `Wire` for a struct with public fields, in the order the fields are listed.
macro_rules! wire_struct {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl Wire for $ty {
            fn write(&self, out: &mut Writer) -> Result<(), WireError> {
                $(self.$field.write(out)?;)*
                Ok(())
            }
            fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
                Ok(Self { $($field: Wire::read(input)?),* })
            }
        }
    };
}

/// Implements `Wire` for an id type that converts to and from 64 bits.
macro_rules! wire_id {
    ($($ty:ty),*) => {$(
        impl Wire for $ty {
            fn write(&self, out: &mut Writer) -> Result<(), WireError> {
                self.to_bits().write(out)
            }
            fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
                Ok(Self::from_bits(u64::read(input)?))
            }
        }
    )*};
}

pub(super) use {wire_id, wire_struct};
