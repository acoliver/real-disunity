//! Packing property values into a Unity constant buffer.
//!
//! The buffer's record, of either form Unity stores it in, is reduced to a [`BufferLayout`],
//! converted to a neutral [`ConstantLayout`] and packed by [`pack_constants`]. Unity's rules
//! live in the conversion: a vector, array element and matrix line each take a 16-byte
//! register, a buffer is a whole number of registers, a matrix of dimension `n` writes `n`
//! registers from the property's floats in the order given, and the one struct array of a
//! player's buffer becomes members whose elements are the struct's size apart.

use std::num::NonZeroU32;

use worldrunner_unity_serialized::shader_parameters as player;
use worldrunner_unity_serialized::shader_program::ConstantBuffer;

use super::buffers::{BufferLayout, Kind, Member as BufferMember};
use super::structs::StructPlacement;

use crate::binding::{BindingSource, PropertyId};
pub use crate::constants::PackedConstants;
use crate::constants::{
    self as neutral, ArrayLayout, ConstantLayout, MatrixLayout, MatrixOrder, Member, ScalarKind,
    pack_constants,
};
use crate::properties::PropertySet;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConstantError {
    #[error("name index {name_index} is not in the pass's names")]
    UnknownName { name_index: i32 },
    #[error("property {property} has a different kind of value than its declaration")]
    WrongKind { property: String },
    #[error("{name} at byte {offset} with {size} bytes does not fit a buffer of {buffer}")]
    OutOfBounds {
        name: String,
        offset: i64,
        size: usize,
        buffer: usize,
    },
    #[error("{name} has a value kind or width this renderer does not write ({kind})")]
    UnsupportedValue { name: String, kind: i32 },
    #[error("{name} declares a negative {field} ({value})")]
    NegativeSize {
        name: String,
        field: &'static str,
        value: i32,
    },
    #[error(
        "struct {name} is not packed: only one array of structs that starts at byte 0 and shares \
         its buffer with no members or structs has been measured"
    )]
    UnsupportedStruct { name: String },
    #[error(
        "struct member {name} at byte {offset} with {size} bytes does not fit its struct of \
         {struct_size}"
    )]
    MemberOutsideStruct {
        name: String,
        offset: i64,
        size: usize,
        struct_size: usize,
    },
    #[error("property {property} has {given} elements, its declaration {declared}")]
    ArrayTooLong {
        property: String,
        declared: usize,
        given: usize,
    },
}

impl From<neutral::ConstantError> for ConstantError {
    fn from(error: neutral::ConstantError) -> Self {
        use neutral::ConstantError as Neutral;
        match error {
            Neutral::WrongKind { property } => Self::WrongKind { property },
            Neutral::OutOfBounds {
                name,
                offset,
                size,
                buffer,
            } => Self::OutOfBounds {
                name,
                offset: i64::try_from(offset).unwrap_or(i64::MAX),
                size: usize::try_from(size).unwrap_or(usize::MAX),
                buffer,
            },
            // Every member the adapter builds is one the packer writes, so this repeats the
            // declaration check as Unity names it.
            Neutral::UnsupportedMember { name } => Self::UnsupportedValue { name, kind: -1 },
            Neutral::ArrayTooLong {
                property,
                declared,
                given,
            } => Self::ArrayTooLong {
                property,
                declared,
                given,
            },
        }
    }
}

/// Bytes of a vector register: HLSL pads every vector, array element and matrix row to it.
const REGISTER: usize = 16;
const REGISTER_BYTES: u32 = 16;

/// Packs a constant buffer of editor-style parameters from the properties its members name by
/// index into `names`. See [`pack_layout`] for how members are written.
///
/// # Errors
/// Fails on a parameter the renderer cannot write, a value of the wrong kind, or a parameter
/// outside the buffer.
pub fn pack_constant_buffer(
    buffer: &ConstantBuffer,
    names: &[(String, i32)],
    layers: &[&PropertySet],
    min_len: usize,
) -> Result<PackedConstants, ConstantError> {
    pack_layout(&BufferLayout::from_program(buffer), names, layers, min_len)
}

/// Packs a constant buffer of a player variant from the properties its members name.
///
/// # Errors
/// As [`pack_constant_buffer`]; a member kind other than float and int is an unsupported value.
pub fn pack_variant_buffer(
    buffer: &player::ConstantBuffer,
    layers: &[&PropertySet],
    min_len: usize,
) -> Result<PackedConstants, ConstantError> {
    pack_layout(&BufferLayout::from_variant(buffer), &[], layers, min_len)
}

/// Packs a buffer from the properties its members name. The buffer holds the larger of the
/// declared size and `min_len` (the size the shader needs), rounded up to a whole register.
///
/// A vector writes its leading `columns` components. Array elements and matrix rows are each a
/// register apart. A matrix of four columns writes four floats for each of its rows, taking
/// the property's floats in the order given. `names` resolves members that name an index.
///
/// A player buffer's struct array is placed by [`BufferLayout::struct_placement`]. A member of
/// the struct is named by its own name, and element `e` of the struct array takes element `e` of
/// the member's array property: a vector, matrix or integer vector array. A single value is
/// element 0's. Elements the property does not reach, and members it does not name, stay zero.
///
/// # Errors
/// Fails on a member the renderer cannot write, a value of the wrong kind, or a member outside
/// the buffer or its struct.
pub fn pack_layout(
    layout: &BufferLayout,
    names: &[(String, i32)],
    layers: &[&PropertySet],
    min_len: usize,
) -> Result<PackedConstants, ConstantError> {
    let neutral = neutral_layout(layout, names, min_len)?;
    Ok(pack_constants(
        &neutral,
        layers,
        min_len.next_multiple_of(REGISTER),
    )?)
}

/// The neutral layout of `layout`, whose size is a whole number of registers. Every member is
/// checked against the buffer here, in member order, so the first member Unity cannot write or
/// place is the error named, whichever way it fails.
fn neutral_layout(
    layout: &BufferLayout,
    names: &[(String, i32)],
    min_len: usize,
) -> Result<ConstantLayout, ConstantError> {
    let placement = layout.struct_placement(names)?;
    let declared = usize::try_from(layout.size).map_err(|_| ConstantError::NegativeSize {
        name: layout.name.resolve(names).unwrap_or("?").to_owned(),
        field: "size",
        value: layout.size,
    })?;
    let registers = declared.next_multiple_of(REGISTER);
    let total = declared.max(min_len).next_multiple_of(REGISTER);
    let mut members = Vec::new();
    for member in &layout.members {
        members.push(self::member(member, None, names, total)?);
    }
    if let Some((structs, placement)) = placement {
        for member in &structs.members {
            members.push(self::member(member, Some(placement), names, total)?);
        }
    }
    Ok(ConstantLayout {
        name: layout.name.resolve(names).unwrap_or("?").to_owned(),
        // `registers` is at most `i32::MAX` rounded up to a register, which fits.
        byte_size: u32::try_from(registers).unwrap_or(u32::MAX),
        members,
    })
}

/// The scalar kind and component count (or matrix row count) when the packer writes the
/// member. A member of a struct is one element of the struct's array, so it is not an array
/// itself, and may be an integer vector of four components, which has been measured only there.
fn written(member: &BufferMember, in_struct: bool) -> Option<(ScalarKind, u8)> {
    let width = |n: i32| u8::try_from(n).ok().filter(|n| (1..=4).contains(n));
    if in_struct && member.array_size > 1 {
        return None;
    }
    match (member.kind, member.matrix) {
        (Kind::Float, false) if member.rows == 1 => {
            width(member.columns).map(|w| (ScalarKind::F32, w))
        }
        (Kind::Float, true) if member.columns == 4 => {
            width(member.rows).map(|w| (ScalarKind::F32, w))
        }
        (Kind::Int, false) if in_struct && member.rows == 1 && member.columns == 4 => {
            Some((ScalarKind::I32, 4))
        }
        (Kind::Int | Kind::Bool | Kind::Uint, false)
            if member.rows == 1 && member.columns == 1 && member.array_size <= 1 =>
        {
            let kind = match member.kind {
                Kind::Int => ScalarKind::I32,
                Kind::Bool => ScalarKind::Bool32,
                _ => ScalarKind::U32,
            };
            Some((kind, 1))
        }
        _ => None,
    }
}

/// The neutral member for a buffer member, placed in the struct array `within` when it belongs
/// to one, or the error Unity names for a declaration the packer cannot write or place.
fn member(
    member: &BufferMember,
    within: Option<StructPlacement>,
    names: &[(String, i32)],
    buffer_len: usize,
) -> Result<Member, ConstantError> {
    let name = member.name.resolve(names)?;
    if member.array_size < 0 {
        return Err(ConstantError::NegativeSize {
            name: name.to_owned(),
            field: "array size",
            value: member.array_size,
        });
    }
    let (kind, width) =
        written(member, within.is_some()).ok_or_else(|| ConstantError::UnsupportedValue {
            name: name.to_owned(),
            kind: member.kind.code(),
        })?;
    // Bytes of one element: a matrix is `width` registers, a vector its components.
    let one = if member.matrix {
        usize::from(width) * REGISTER
    } else {
        usize::from(width) * 4
    };
    let offset = i64::from(member.offset);
    let (start, count, stride) = match within {
        None => (
            offset,
            usize::try_from(member.array_size).unwrap_or(0).max(1),
            if member.matrix { one } else { REGISTER },
        ),
        Some(placement) => {
            let fits = usize::try_from(offset).is_ok_and(|o| o + one <= placement.stride);
            if !fits {
                return Err(ConstantError::MemberOutsideStruct {
                    name: name.to_owned(),
                    offset,
                    size: one,
                    struct_size: placement.stride,
                });
            }
            (
                offset + i64::try_from(placement.start).unwrap_or(i64::MAX),
                placement.count,
                placement.stride,
            )
        }
    };
    let extent = (count - 1) * stride + one;
    let end = usize::try_from(start)
        .ok()
        .and_then(|start| start.checked_add(extent));
    if end.is_none_or(|end| end > buffer_len) {
        return Err(ConstantError::OutOfBounds {
            name: name.to_owned(),
            offset: start,
            size: extent,
            buffer: buffer_len,
        });
    }
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    Ok(Member {
        source: BindingSource::Property(PropertyId::new(name)),
        kind,
        rows: if member.matrix { 4 } else { 1 },
        cols: width,
        array: (count > 1).then(|| ArrayLayout {
            count: NonZeroU32::new(count).unwrap_or(NonZeroU32::MIN),
            stride: u32::try_from(stride).unwrap_or(u32::MAX),
        }),
        offset: u32::try_from(start).unwrap_or(u32::MAX),
        matrix: member.matrix.then_some(MatrixLayout {
            order: MatrixOrder::ColumnMajor,
            stride: REGISTER_BYTES,
        }),
    })
}
