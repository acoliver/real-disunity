//! Packing property values into a constant buffer.

use worldrunner_unity_serialized::shader_program::{ConstantBuffer, ValueParam};

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
    UnsupportedValue { name: String, kind: i8 },
    #[error("struct member {name} is not packed")]
    UnsupportedStruct { name: String },
    #[error("property {property} has {given} elements, its declaration {declared}")]
    ArrayTooLong {
        property: String,
        declared: usize,
        given: usize,
    },
}

/// Unity's `ShaderParamType` values this packer writes.
const KIND_FLOAT: i8 = 0;
const KIND_INT: i8 = 1;
const KIND_BOOL: i8 = 2;
const KIND_UINT: i8 = 5;

/// Bytes of a vector register: HLSL pads every vector, array element and matrix row to it.
const REGISTER: usize = 16;

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
    if let Some(member) = buffer.structs.first() {
        return Err(ConstantError::UnsupportedStruct {
            name: name_of(names, member.name_index)?.to_owned(),
        });
    }
    let declared = usize::try_from(buffer.size).unwrap_or(0);
    let mut packer = Packer {
        names,
        layers,
        bytes: vec![0; declared.max(min_len).next_multiple_of(REGISTER)],
        missing: Vec::new(),
    };
    for param in &buffer.vectors {
        packer.vector(param)?;
    }
    for param in &buffer.matrices {
        packer.matrix(param)?;
    }
    Ok(PackedConstants {
        bytes: packer.bytes,
        missing: packer.missing,
    })
}

fn name_of(names: &[(String, i32)], name_index: i32) -> Result<&str, ConstantError> {
    names
        .iter()
        .find(|(_, index)| *index == name_index)
        .map(|(name, _)| name.as_str())
        .ok_or(ConstantError::UnknownName { name_index })
}

struct Packer<'a> {
    names: &'a [(String, i32)],
    layers: &'a [&'a PropertySet],
    bytes: Vec<u8>,
    missing: Vec<String>,
}

/// A parameter's name, value and declaration, once its property is found.
struct Found<'p> {
    name: String,
    value: PropertyValue,
    param: &'p ValueParam,
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
            kind: self.param.kind,
        }
    }

    /// How many elements the parameter declares.
    fn declared(&self) -> usize {
        usize::try_from(self.param.array_size).unwrap_or(0).max(1)
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

impl Packer<'_> {
    /// The parameter's property, or `None` after recording it as missing.
    fn find<'p>(&mut self, param: &'p ValueParam) -> Result<Option<Found<'p>>, ConstantError> {
        let name = name_of(self.names, param.name_index)?.to_owned();
        let Some(value) = lookup(self.layers, &name).cloned() else {
            if !self.missing.contains(&name) {
                self.missing.push(name);
            }
            return Ok(None);
        };
        Ok(Some(Found { name, value, param }))
    }

    /// Copies `data` to `offset`, which must lie inside the buffer.
    fn store(
        &mut self,
        found: &Found<'_>,
        offset: usize,
        data: &[u8],
    ) -> Result<(), ConstantError> {
        let start = usize::try_from(found.param.index)
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
                offset: i64::from(found.param.index) + i64::try_from(offset).unwrap_or(i64::MAX),
                size: data.len(),
                buffer: self.bytes.len(),
            }),
        }
    }

    /// Rejects a declaration this packer cannot write, whether or not a value exists.
    fn check_declared(&self, param: &ValueParam, matrix: bool) -> Result<(), ConstantError> {
        let width_ok = (1..=4).contains(&param.dimension);
        let scalar_ok = param.dimension == 1 && param.array_size <= 1;
        let supported = match param.kind {
            KIND_FLOAT => width_ok,
            KIND_INT | KIND_BOOL | KIND_UINT => scalar_ok && !matrix,
            _ => false,
        };
        let name = name_of(self.names, param.name_index)?;
        if !supported {
            return Err(ConstantError::UnsupportedValue {
                name: name.to_owned(),
                kind: param.kind,
            });
        }
        let width = usize::try_from(param.dimension).unwrap_or(0);
        let count = usize::try_from(param.array_size).unwrap_or(0).max(1);
        let extent = if matrix {
            count * width * REGISTER
        } else {
            (count - 1) * REGISTER + width * 4
        };
        let end = usize::try_from(param.index)
            .ok()
            .and_then(|start| start.checked_add(extent));
        if end.is_some_and(|end| end <= self.bytes.len()) {
            return Ok(());
        }
        Err(ConstantError::OutOfBounds {
            name: name.to_owned(),
            offset: i64::from(param.index),
            size: extent,
            buffer: self.bytes.len(),
        })
    }

    fn vector(&mut self, param: &ValueParam) -> Result<(), ConstantError> {
        self.check_declared(param, false)?;
        let Some(found) = self.find(param)? else {
            return Ok(());
        };
        let width = usize::try_from(param.dimension)
            .ok()
            .filter(|width| (1..=4).contains(width));
        let (Some(width), KIND_FLOAT) = (width, param.kind) else {
            return self.scalar(&found);
        };
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
            self.store(&found, index * REGISTER, &data)?;
        }
        Ok(())
    }

    /// An integer, unsigned or boolean scalar; every other kind and width is unsupported.
    fn scalar(&mut self, found: &Found<'_>) -> Result<(), ConstantError> {
        let param = found.param;
        if param.dimension != 1 || param.array_size > 1 {
            return Err(found.unsupported());
        }
        let PropertyValue::Int(value) = found.value else {
            return match param.kind {
                KIND_INT | KIND_BOOL | KIND_UINT => Err(found.wrong_kind()),
                _ => Err(found.unsupported()),
            };
        };
        let bytes = match param.kind {
            KIND_INT => value.to_le_bytes(),
            KIND_BOOL if value == 0 || value == 1 => value.to_le_bytes(),
            KIND_UINT => u32::try_from(value)
                .map_err(|_| found.wrong_kind())?
                .to_le_bytes(),
            KIND_BOOL => return Err(found.wrong_kind()),
            _ => return Err(found.unsupported()),
        };
        self.store(found, 0, &bytes)
    }

    fn matrix(&mut self, param: &ValueParam) -> Result<(), ConstantError> {
        self.check_declared(param, true)?;
        let Some(found) = self.find(param)? else {
            return Ok(());
        };
        let rows = usize::try_from(param.dimension)
            .ok()
            .filter(|rows| (1..=4).contains(rows));
        let (Some(rows), KIND_FLOAT) = (rows, param.kind) else {
            return Err(found.unsupported());
        };
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
            self.store(&found, index * rows * REGISTER, &data)?;
        }
        Ok(())
    }
}
