//! Packing property values into a constant buffer.

use worldrunner_unity_serialized::shader_parameters as player;
use worldrunner_unity_serialized::shader_program::ConstantBuffer;

use crate::buffers::{BufferLayout, Kind, Member, ParamName, StructArray};
use crate::properties::{PropertySet, PropertyValue, lookup};
use crate::structs::StructPlacement;

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
    let placement = layout.struct_placement(names)?;
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
        packer.member(member, None)?;
    }
    if let Some((structs, placement)) = placement {
        packer.struct_array(structs, placement)?;
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

/// Where a member's elements sit: the first one's byte offset, how many there are and how far
/// apart.
#[derive(Clone, Copy)]
struct Elements {
    offset: i64,
    count: usize,
    stride: usize,
}

/// A member's name, value and declaration, once its property is found.
struct Found<'m> {
    name: String,
    value: PropertyValue,
    member: &'m Member,
    elements: Elements,
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

    fn check_length(&self, given: usize) -> Result<(), ConstantError> {
        if given > self.elements.count {
            return Err(ConstantError::ArrayTooLong {
                property: self.name.clone(),
                declared: self.elements.count,
                given,
            });
        }
        Ok(())
    }
}

/// The vector component count, or the matrix row count, when the packer writes the member. A
/// member of a struct is one element of the struct's array, so it is not an array itself, and
/// may be an integer vector of four components, which has been measured only there.
fn written_width(member: &Member, in_struct: bool) -> Option<usize> {
    let width = |n: i32| usize::try_from(n).ok().filter(|n| (1..=4).contains(n));
    if in_struct && member.array_size > 1 {
        return None;
    }
    match (member.kind, member.matrix) {
        (Kind::Float, false) if member.rows == 1 => width(member.columns),
        (Kind::Float, true) if member.columns == 4 => width(member.rows),
        (Kind::Int, false) if in_struct && member.rows == 1 && member.columns == 4 => Some(4),
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
    fn find<'m>(
        &mut self,
        member: &'m Member,
        elements: Elements,
    ) -> Result<Option<Found<'m>>, ConstantError> {
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
            elements,
        }))
    }

    /// Copies `data` to element `index` of the member, inside the buffer.
    fn store(&mut self, found: &Found<'_>, index: usize, data: &[u8]) -> Result<(), ConstantError> {
        let elements = found.elements;
        let at = i64::try_from(index * elements.stride)
            .ok()
            .and_then(|spacing| elements.offset.checked_add(spacing));
        let start = at.and_then(|at| usize::try_from(at).ok());
        let end = start.and_then(|start| start.checked_add(data.len()));
        match (start, end) {
            (Some(start), Some(end)) if end <= self.bytes.len() => {
                self.bytes[start..end].copy_from_slice(data);
                Ok(())
            }
            _ => Err(ConstantError::OutOfBounds {
                name: found.name.clone(),
                offset: at.unwrap_or(i64::MAX),
                size: data.len(),
                buffer: self.bytes.len(),
            }),
        }
    }

    /// Rejects a declaration this packer cannot write, whether or not a value exists, and
    /// places the member's elements: `within` is the struct array the member belongs to.
    fn check_declared(
        &self,
        member: &Member,
        within: Option<StructPlacement>,
    ) -> Result<(usize, Elements), ConstantError> {
        let name = self.name(&member.name)?;
        if member.array_size < 0 {
            return Err(ConstantError::NegativeSize {
                name: name.to_owned(),
                field: "array size",
                value: member.array_size,
            });
        }
        let Some(width) = written_width(member, within.is_some()) else {
            return Err(ConstantError::UnsupportedValue {
                name: name.to_owned(),
                kind: member.kind.code(),
            });
        };
        let one = if member.matrix {
            width * REGISTER
        } else {
            width * 4
        };
        let offset = i64::from(member.offset);
        let elements = match within {
            None => Elements {
                offset,
                count: usize::try_from(member.array_size).unwrap_or(0).max(1),
                stride: if member.matrix { one } else { REGISTER },
            },
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
                Elements {
                    offset: offset + i64::try_from(placement.start).unwrap_or(i64::MAX),
                    count: placement.count,
                    stride: placement.stride,
                }
            }
        };
        let extent = (elements.count - 1) * elements.stride + one;
        let end = usize::try_from(elements.offset)
            .ok()
            .and_then(|start| start.checked_add(extent));
        if end.is_some_and(|end| end <= self.bytes.len()) {
            return Ok((width, elements));
        }
        Err(ConstantError::OutOfBounds {
            name: name.to_owned(),
            offset: elements.offset,
            size: extent,
            buffer: self.bytes.len(),
        })
    }

    fn struct_array(
        &mut self,
        structs: &StructArray,
        placement: StructPlacement,
    ) -> Result<(), ConstantError> {
        for member in &structs.members {
            self.member(member, Some(placement))?;
        }
        Ok(())
    }

    fn member(
        &mut self,
        member: &Member,
        within: Option<StructPlacement>,
    ) -> Result<(), ConstantError> {
        let (width, elements) = self.check_declared(member, within)?;
        let Some(found) = self.find(member, elements)? else {
            return Ok(());
        };
        match (member.kind, member.matrix) {
            (_, true) => self.matrix(&found, width),
            (Kind::Float, false) => self.vector(&found, width),
            (Kind::Int, false) => self.integers(&found, width),
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
            self.store(found, index, &data)?;
        }
        Ok(())
    }

    /// A signed integer scalar or vector, written as the leading `width` components.
    fn integers(&mut self, found: &Found<'_>, width: usize) -> Result<(), ConstantError> {
        let elements: Vec<[i32; 4]> = match &found.value {
            PropertyValue::Int(x) if width == 1 => vec![[*x, 0, 0, 0]],
            PropertyValue::IntVector(v) => vec![*v],
            PropertyValue::IntVectorArray(items) => items.clone(),
            _ => return Err(found.wrong_kind()),
        };
        found.check_length(elements.len())?;
        for (index, element) in elements.iter().enumerate() {
            let data: Vec<u8> = element[..width]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            self.store(found, index, &data)?;
        }
        Ok(())
    }

    /// A boolean or unsigned scalar; every other kind and width is unsupported.
    fn scalar(&mut self, found: &Found<'_>) -> Result<(), ConstantError> {
        let PropertyValue::Int(value) = found.value else {
            return match found.member.kind {
                Kind::Int | Kind::Bool | Kind::Uint => Err(found.wrong_kind()),
                _ => Err(found.unsupported()),
            };
        };
        let bytes = match found.member.kind {
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
            self.store(found, index, &data)?;
        }
        Ok(())
    }
}
