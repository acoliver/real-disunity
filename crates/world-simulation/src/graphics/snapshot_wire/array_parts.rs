//! The encoding of vector and matrix array properties.
//!
//! An array is one element count (1 to [`MAX_PROPERTY_ARRAY_ELEMENTS`]) and then that many
//! elements of IEEE little-endian floats, every one finite. The reader takes the payload bytes
//! before it allocates the typed elements, so a count that the input cannot fill allocates
//! nothing. A list of named arrays holds its names strictly ascending, which is how a set of
//! properties stores them, so one value has one encoding.
use super::WireError;
use super::codec::{MAX_ITEMS, Reader, Wire, Writer};
use crate::graphics::{MAX_NAME_BYTES, MAX_PROPERTY_ARRAY_ELEMENTS};

/// The named arrays of one kind in a set of properties, `N` floats to an element.
type Named<const N: usize> = Vec<(String, Vec<[f32; N]>)>;

const MAX_ELEMENTS: u32 = 1023;
const MAX_NAME: u32 = 256;

const _: () = assert!(MAX_ELEMENTS as usize == MAX_PROPERTY_ARRAY_ELEMENTS);
const _: () = assert!(MAX_NAME as usize == MAX_NAME_BYTES);

/// The fewest bytes one named array of `N`-float elements takes: the name length, one name byte,
/// the element count and one element.
const fn min_entry_bytes<const N: usize>() -> usize {
    4 + 1 + 4 + N * 4
}

fn finite(values: &[f32]) -> Result<(), WireError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(WireError::InvalidPropertyArray)
    }
}

/// Writes the name of an array property, which is 1 to 256 bytes.
pub(super) fn write_array_name(out: &mut Writer, name: &str) -> Result<(), WireError> {
    if name.is_empty() {
        return Err(WireError::InvalidPropertyArray);
    }
    out.count(name.len(), MAX_NAME)?;
    out.raw(name.as_bytes());
    Ok(())
}

/// Reads a name that [`write_array_name`] wrote.
pub(super) fn read_array_name(input: &mut Reader<'_>) -> Result<String, WireError> {
    let length = input.count(MAX_NAME)?;
    if length == 0 {
        return Err(WireError::InvalidPropertyArray);
    }
    String::from_utf8(input.take(length)?.to_vec()).map_err(|_| WireError::BadText)
}

/// Writes the elements of an array property.
pub(super) fn write_array<const N: usize>(
    out: &mut Writer,
    values: &[[f32; N]],
) -> Result<(), WireError> {
    if values.is_empty() {
        return Err(WireError::InvalidPropertyArray);
    }
    out.count(values.len(), MAX_ELEMENTS)?;
    for element in values {
        finite(element)?;
        element.iter().try_for_each(|value| value.write(out))?;
    }
    Ok(())
}

/// Reads the elements that [`write_array`] wrote.
pub(super) fn read_array<const N: usize>(
    input: &mut Reader<'_>,
) -> Result<Vec<[f32; N]>, WireError> {
    let count = input.count(MAX_ELEMENTS)?;
    if count == 0 {
        return Err(WireError::InvalidPropertyArray);
    }
    let bytes = count.checked_mul(N * 4).ok_or(WireError::TooLong)?;
    let payload = input.take(bytes)?;
    let mut elements = Vec::new();
    elements
        .try_reserve_exact(count)
        .map_err(|_| WireError::AllocationFailed)?;
    for chunk in payload.chunks_exact(N * 4) {
        let mut element = [0.0_f32; N];
        for (slot, bytes) in element.iter_mut().zip(chunk.chunks_exact(4)) {
            let bytes = <[u8; 4]>::try_from(bytes).map_err(|_| WireError::Truncated)?;
            *slot = f32::from_le_bytes(bytes);
        }
        finite(&element)?;
        elements.push(element);
    }
    Ok(elements)
}

/// Writes a list of named arrays, which is sorted by name without a name twice.
pub(super) fn write_array_list<const N: usize>(
    out: &mut Writer,
    list: &[(String, Vec<[f32; N]>)],
) -> Result<(), WireError> {
    out.count(list.len(), MAX_ITEMS)?;
    let mut previous: Option<&str> = None;
    for (name, values) in list {
        if previous.is_some_and(|before| before >= name.as_str()) {
            return Err(WireError::InvalidPropertyArray);
        }
        previous = Some(name);
        write_array_name(out, name)?;
        write_array(out, values)?;
    }
    Ok(())
}

/// Reads a list that [`write_array_list`] wrote. The number of entries is checked against the
/// bytes that are left before the list is reserved.
pub(super) fn read_array_list<const N: usize>(
    input: &mut Reader<'_>,
) -> Result<Named<N>, WireError> {
    let count = input.count(MAX_ITEMS)?;
    let least = count
        .checked_mul(min_entry_bytes::<N>())
        .ok_or(WireError::TooLong)?;
    if least > input.remaining() {
        return Err(WireError::Truncated);
    }
    let mut list = Named::<N>::new();
    list.try_reserve_exact(count)
        .map_err(|_| WireError::AllocationFailed)?;
    for _ in 0..count {
        let name = read_array_name(input)?;
        if list.last().is_some_and(|(before, _)| *before >= name) {
            return Err(WireError::InvalidPropertyArray);
        }
        list.push((name, read_array(input)?));
    }
    Ok(list)
}
