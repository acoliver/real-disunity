//! Packing property values into a Unity constant buffer.
//!
//! The buffer's record is converted to a neutral [`ConstantLayout`] and packed by
//! [`pack_constants`]. Unity's rules live in the conversion: a vector, array element and matrix
//! line each take a 16-byte register, a buffer is a whole number of registers, and a matrix of
//! dimension `n` writes `n` registers from the property's floats in the order given.

use std::num::NonZeroU32;

use worldrunner_unity_serialized::shader_program::{ConstantBuffer, ValueParam};

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
    UnsupportedValue { name: String, kind: i8 },
    #[error("{name} declares a negative {field} ({value})")]
    NegativeSize {
        name: String,
        field: &'static str,
        value: i32,
    },
    #[error("struct member {name} is not packed")]
    UnsupportedStruct { name: String },
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

/// Unity's `ShaderParamType` values this packer writes.
const KIND_FLOAT: i8 = 0;
const KIND_INT: i8 = 1;
const KIND_BOOL: i8 = 2;
const KIND_UINT: i8 = 5;

/// Bytes of a vector register: HLSL pads every vector, array element and matrix row to it.
const REGISTER: usize = 16;
const REGISTER_BYTES: u32 = 16;

/// Packs a constant buffer from the properties its parameters name. The buffer holds the
/// larger of the declared size and `min_len` (the size the shader needs), rounded up to a
/// whole register.
///
/// A vector writes its leading `dimension` components. Array elements and matrix rows are
/// each a register apart. A matrix writes four floats for each of `dimension` rows, taking the
/// property's floats in the order given.
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
    let layout = layout(buffer, names, min_len)?;
    Ok(pack_constants(
        &layout,
        layers,
        min_len.next_multiple_of(REGISTER),
    )?)
}

/// The neutral layout of `buffer`, whose size is a whole number of registers.
fn layout(
    buffer: &ConstantBuffer,
    names: &[(String, i32)],
    min_len: usize,
) -> Result<ConstantLayout, ConstantError> {
    if let Some(member) = buffer.structs.first() {
        return Err(ConstantError::UnsupportedStruct {
            name: name_of(names, member.name_index)?.to_owned(),
        });
    }
    let declared = usize::try_from(buffer.size).map_err(|_| ConstantError::NegativeSize {
        name: name_of(names, buffer.name_index).unwrap_or("?").to_owned(),
        field: "size",
        value: buffer.size,
    })?;
    let registers = declared.next_multiple_of(REGISTER);
    let total = declared.max(min_len).next_multiple_of(REGISTER);
    let mut members = Vec::new();
    for param in &buffer.vectors {
        members.push(member(param, false, names, total)?);
    }
    for param in &buffer.matrices {
        members.push(member(param, true, names, total)?);
    }
    Ok(ConstantLayout {
        name: name_of(names, buffer.name_index).unwrap_or("?").to_owned(),
        // `registers` is at most `i32::MAX` rounded up to a register, which fits.
        byte_size: u32::try_from(registers).unwrap_or(u32::MAX),
        members,
    })
}

fn name_of(names: &[(String, i32)], name_index: i32) -> Result<&str, ConstantError> {
    names
        .iter()
        .find(|(_, index)| *index == name_index)
        .map(|(name, _)| name.as_str())
        .ok_or(ConstantError::UnknownName { name_index })
}

/// The neutral member for a parameter, or the error Unity names for a declaration the packer
/// cannot write.
fn member(
    param: &ValueParam,
    matrix: bool,
    names: &[(String, i32)],
    buffer_len: usize,
) -> Result<Member, ConstantError> {
    let name = name_of(names, param.name_index)?;
    if param.array_size < 0 {
        return Err(ConstantError::NegativeSize {
            name: name.to_owned(),
            field: "array size",
            value: param.array_size,
        });
    }
    let (kind, dimension) =
        supported(param, matrix).ok_or_else(|| ConstantError::UnsupportedValue {
            name: name.to_owned(),
            kind: param.kind,
        })?;
    let count = u32::try_from(param.array_size).unwrap_or(0).max(1);
    let registers_per_element = if matrix { u32::from(dimension) } else { 1 };
    let array = (count > 1).then(|| ArrayLayout {
        count: NonZeroU32::new(count).unwrap_or(NonZeroU32::MIN),
        stride: REGISTER_BYTES * registers_per_element,
    });
    let built = Member {
        source: BindingSource::Property(PropertyId::new(name)),
        kind,
        rows: if matrix { 4 } else { 1 },
        cols: dimension,
        array,
        offset: u32::try_from(param.index).unwrap_or(0),
        matrix: matrix.then_some(MatrixLayout {
            order: MatrixOrder::ColumnMajor,
            stride: REGISTER_BYTES,
        }),
    };
    if param.index < 0 {
        return Err(ConstantError::OutOfBounds {
            name: name.to_owned(),
            offset: i64::from(param.index),
            size: usize::try_from(built.extent()).unwrap_or(usize::MAX),
            buffer: buffer_len,
        });
    }
    Ok(built)
}

/// The scalar kind and dimension of a parameter the packer writes. A float is a vector of 1 to
/// 4 components or a matrix of 1 to 4 registers; an integer, unsigned integer or boolean is a
/// single value.
fn supported(param: &ValueParam, matrix: bool) -> Option<(ScalarKind, u8)> {
    let width = u8::try_from(param.dimension)
        .ok()
        .filter(|w| (1..=4).contains(w))?;
    let scalar = width == 1 && param.array_size <= 1 && !matrix;
    match param.kind {
        KIND_FLOAT => Some((ScalarKind::F32, width)),
        KIND_INT if scalar => Some((ScalarKind::I32, 1)),
        KIND_BOOL if scalar => Some((ScalarKind::Bool32, 1)),
        KIND_UINT if scalar => Some((ScalarKind::U32, 1)),
        _ => None,
    }
}
