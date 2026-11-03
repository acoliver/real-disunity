//! Packing property values into a constant buffer.

use worldrunner_unity_serialized::shader_parameters as player;
use worldrunner_unity_serialized::shader_program::ConstantBuffer;

use crate::buffers::{BufferLayout, Kind, Member, ParamName};
use crate::properties::{PropertySet, PropertyValue, lookup};

/// A constant buffer's bytes and the properties that had no value.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedConstants {
    pub bytes: Vec<u8>,
    pub missing: Vec<String>,
}

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
    #[error("struct member {name} is not packed")]
    UnsupportedStruct { name: String },
    #[error("property {property} has {given} elements, its declaration {declared}")]
    ArrayTooLong {
        property: String,
        declared: usize,
        given: usize,
    },
}

/// Bytes of a vector register: HLSL pads every vector, array element and matrix row to it.
const REGISTER: usize = 16;

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
/// # Errors
/// Fails on a member the renderer cannot write, a value of the wrong kind, or a member outside
/// the buffer.
pub fn pack_layout(
    layout: &BufferLayout,
    names: &[(String, i32)],
    layers: &[&PropertySet],
    min_len: usize,
) -> Result<PackedConstants, ConstantError> {
    if let Some(name) = layout.structs.first() {
        return Err(ConstantError::UnsupportedStruct {
            name: name.resolve(names)?.to_owned(),
        });
    }
    let declared = usize::try_from(layout.size).map_err(|_| ConstantError::NegativeSize {
        name: layout.name.resolve(names).unwrap_or("?").to_owned(),
        field: "size",
        value: layout.size,
    })?;
    let mut packer = Packer {
        names,
        layers,
        bytes: vec![0; declared.max(min_len).next_multiple_of(REGISTER)],
        missing: Vec::new(),
    };
    for member in &layout.members {
        packer.member(member)?;
    }
    Ok(PackedConstants {
        bytes: packer.bytes,
        missing: packer.missing,
    })
}

struct Packer<'a> {
    names: &'a [(String, i32)],
    layers: &'a [&'a PropertySet],
    bytes: Vec<u8>,
    missing: Vec<String>,
}

/// A member's name, value and declaration, once its property is found.
struct Found<'m> {
    name: String,
    value: PropertyValue,
    member: &'m Member,
}

impl Found<'_> {
    fn wrong_kind(&self) -> ConstantError {
        ConstantError::WrongKind {
            property: self.name.clone(),
        }
    }

    fn unsupported(&self) -> ConstantError {
        ConstantError::UnsupportedValue {
            name: self.name.clone(),
            kind: self.member.kind.code(),
        }
    }

    /// How many elements the member declares. Unity writes 0 for a member that is not an
    /// array; `check_declared` has already rejected a negative size.
    fn declared(&self) -> usize {
        usize::try_from(self.member.array_size).unwrap_or(0).max(1)
    }

    fn check_length(&self, given: usize) -> Result<(), ConstantError> {
        if given > self.declared() {
            return Err(ConstantError::ArrayTooLong {
                property: self.name.clone(),
                declared: self.declared(),
                given,
            });
        }
        Ok(())
    }
}

/// The vector component count, or the matrix row count, when the packer writes the member.
fn written_width(member: &Member) -> Option<usize> {
    let width = |n: i32| usize::try_from(n).ok().filter(|n| (1..=4).contains(n));
    match (member.kind, member.matrix) {
        (Kind::Float, false) if member.rows == 1 => width(member.columns),
        (Kind::Float, true) if member.columns == 4 => width(member.rows),
        (Kind::Int | Kind::Bool | Kind::Uint, false)
            if member.rows == 1 && member.columns == 1 && member.array_size <= 1 =>
        {
            Some(1)
        }
        _ => None,
    }
}

impl Packer<'_> {
    fn name<'n>(&'n self, name: &'n ParamName) -> Result<&'n str, ConstantError> {
        name.resolve(self.names)
    }

    /// The member's property, or `None` after recording it as missing.
    fn find<'m>(&mut self, member: &'m Member) -> Result<Option<Found<'m>>, ConstantError> {
        let name = self.name(&member.name)?.to_owned();
        let Some(value) = lookup(self.layers, &name).cloned() else {
            if !self.missing.contains(&name) {
                self.missing.push(name);
            }
            return Ok(None);
        };
        Ok(Some(Found {
            name,
            value,
            member,
        }))
    }

    /// Copies `data` to `offset` bytes past the member's offset, inside the buffer.
    fn store(
        &mut self,
        found: &Found<'_>,
        offset: usize,
        data: &[u8],
    ) -> Result<(), ConstantError> {
        let start = usize::try_from(found.member.offset)
            .ok()
            .and_then(|base| base.checked_add(offset));
        let end = start.and_then(|start| start.checked_add(data.len()));
        match (start, end) {
            (Some(start), Some(end)) if end <= self.bytes.len() => {
                self.bytes[start..end].copy_from_slice(data);
                Ok(())
            }
            _ => Err(ConstantError::OutOfBounds {
                name: found.name.clone(),
                offset: i64::from(found.member.offset) + i64::try_from(offset).unwrap_or(i64::MAX),
                size: data.len(),
                buffer: self.bytes.len(),
            }),
        }
    }

    /// Rejects a declaration this packer cannot write, whether or not a value exists.
    fn check_declared(&self, member: &Member) -> Result<usize, ConstantError> {
        let name = self.name(&member.name)?;
        if member.array_size < 0 {
            return Err(ConstantError::NegativeSize {
                name: name.to_owned(),
                field: "array size",
                value: member.array_size,
            });
        }
        let Some(width) = written_width(member) else {
            return Err(ConstantError::UnsupportedValue {
                name: name.to_owned(),
                kind: member.kind.code(),
            });
        };
        let count = usize::try_from(member.array_size).unwrap_or(0).max(1);
        let extent = if member.matrix {
            count * width * REGISTER
        } else {
            (count - 1) * REGISTER + width * 4
        };
        let end = usize::try_from(member.offset)
            .ok()
            .and_then(|start| start.checked_add(extent));
        if end.is_some_and(|end| end <= self.bytes.len()) {
            return Ok(width);
        }
        Err(ConstantError::OutOfBounds {
            name: name.to_owned(),
            offset: i64::from(member.offset),
            size: extent,
            buffer: self.bytes.len(),
        })
    }

    fn member(&mut self, member: &Member) -> Result<(), ConstantError> {
        let width = self.check_declared(member)?;
        let Some(found) = self.find(member)? else {
            return Ok(());
        };
        match (member.kind, member.matrix) {
            (_, true) => self.matrix(&found, width),
            (Kind::Float, false) => self.vector(&found, width),
            _ => self.scalar(&found),
        }
    }

    fn vector(&mut self, found: &Found<'_>, width: usize) -> Result<(), ConstantError> {
        let elements: Vec<[f32; 4]> = match &found.value {
            PropertyValue::Float(x) if width == 1 => vec![[*x, 0.0, 0.0, 0.0]],
            PropertyValue::Vector(v) => vec![*v],
            PropertyValue::VectorArray(items) => items.clone(),
            _ => return Err(found.wrong_kind()),
        };
        found.check_length(elements.len())?;
        for (index, element) in elements.iter().enumerate() {
            let data: Vec<u8> = element[..width]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            self.store(found, index * REGISTER, &data)?;
        }
        Ok(())
    }

    /// An integer, unsigned or boolean scalar; every other kind and width is unsupported.
    fn scalar(&mut self, found: &Found<'_>) -> Result<(), ConstantError> {
        let PropertyValue::Int(value) = found.value else {
            return match found.member.kind {
                Kind::Int | Kind::Bool | Kind::Uint => Err(found.wrong_kind()),
                _ => Err(found.unsupported()),
            };
        };
        let bytes = match found.member.kind {
            Kind::Int => value.to_le_bytes(),
            Kind::Bool if value == 0 || value == 1 => value.to_le_bytes(),
            Kind::Uint => u32::try_from(value)
                .map_err(|_| found.wrong_kind())?
                .to_le_bytes(),
            Kind::Bool => return Err(found.wrong_kind()),
            _ => return Err(found.unsupported()),
        };
        self.store(found, 0, &bytes)
    }

    fn matrix(&mut self, found: &Found<'_>, rows: usize) -> Result<(), ConstantError> {
        let elements: Vec<[f32; 16]> = match &found.value {
            PropertyValue::Matrix(m) => vec![*m],
            PropertyValue::MatrixArray(items) => items.clone(),
            _ => return Err(found.wrong_kind()),
        };
        found.check_length(elements.len())?;
        for (index, element) in elements.iter().enumerate() {
            let data: Vec<u8> = element[..rows * 4]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            self.store(found, index * rows * REGISTER, &data)?;
        }
        Ok(())
    }
}
