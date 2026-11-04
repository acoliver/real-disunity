//! Constant buffer layouts and packing property values into them.
//!
//! A layout says where each named member lives: its scalar kind, shape, byte offset and the
//! strides of its array elements and matrix lines. Nothing here assumes a shader language's
//! packing rule, so a frontend states the strides its shaders use.

use std::num::NonZeroU32;

use crate::binding::{BindingAddress, BindingSource};
use crate::properties::{PropertySet, PropertyValue, lookup};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarKind {
    F32,
    I32,
    U32,
    /// Stored as 0 or 1 in four bytes.
    Bool32,
}

/// Which line of a matrix is contiguous in the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixOrder {
    /// Each row is a line: `rows` lines of `cols` floats.
    RowMajor,
    /// Each column is a line: `cols` lines of `rows` floats.
    ColumnMajor,
}

/// The elements of an array member: how many, and the bytes from one element to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayLayout {
    pub count: NonZeroU32,
    pub stride: u32,
}

/// How a matrix member's lines are stored, `stride` bytes apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixLayout {
    pub order: MatrixOrder,
    pub stride: u32,
}

/// One named value in a constant buffer.
///
/// A vector (`matrix` is `None`) has one row and `cols` components. A matrix has `rows` by
/// `cols` logical values, which [`PropertyValue::Matrix`] holds at index `column * 4 + row`.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub source: BindingSource,
    pub kind: ScalarKind,
    pub rows: u8,
    pub cols: u8,
    pub array: Option<ArrayLayout>,
    /// Bytes from the start of the buffer to the first element.
    pub offset: u32,
    pub matrix: Option<MatrixLayout>,
}

impl Member {
    /// Bytes from the member's offset to the end of the last value written.
    #[must_use]
    pub fn extent(&self) -> u64 {
        let (rows, cols) = (u64::from(self.rows), u64::from(self.cols));
        let value = match self.matrix {
            None => cols * 4,
            Some(MatrixLayout { order, stride }) => {
                let (lines, line_len) = match order {
                    MatrixOrder::ColumnMajor => (cols, rows),
                    MatrixOrder::RowMajor => (rows, cols),
                };
                (lines.saturating_sub(1)) * u64::from(stride) + line_len * 4
            }
        };
        let before_last = self.array.map_or(0, |array| {
            u64::from(array.count.get() - 1) * u64::from(array.stride)
        });
        value + before_last
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstantLayout {
    pub name: String,
    /// The declared size. A buffer is at least this long.
    pub byte_size: u32,
    pub members: Vec<Member>,
}

/// A constant buffer layout and the binding it fills.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundConstantLayout {
    pub at: BindingAddress,
    pub layout: ConstantLayout,
}

/// A constant buffer's bytes and the properties that had no value.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedConstants {
    pub bytes: Vec<u8>,
    pub missing: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConstantError {
    #[error("property {property} has a different kind of value than its declaration")]
    WrongKind { property: String },
    #[error("{name} at byte {offset} with {size} bytes does not fit a buffer of {buffer}")]
    OutOfBounds {
        name: String,
        offset: u64,
        size: u64,
        buffer: usize,
    },
    #[error("{name} has a kind or shape this packer does not write")]
    UnsupportedMember { name: String },
    #[error("property {property} has {given} elements, its declaration {declared}")]
    ArrayTooLong {
        property: String,
        declared: usize,
        given: usize,
    },
}

/// Packs `layout` from the first of `layers` that has each member's property. The buffer holds
/// the larger of the layout's size and `min_len`, which is the size the shader needs.
///
/// A member with no value stays zero and is reported in `missing`.
///
/// # Errors
/// Fails on a member shape the packer does not write, a member outside the buffer, a value of
/// another kind than its member and an array value longer than its member.
pub fn pack_constants(
    layout: &ConstantLayout,
    layers: &[&PropertySet],
    min_len: usize,
) -> Result<PackedConstants, ConstantError> {
    let declared = usize::try_from(layout.byte_size).unwrap_or(usize::MAX);
    let mut packer = Packer {
        layers,
        bytes: vec![0; declared.max(min_len)],
        missing: Vec::new(),
    };
    for member in &layout.members {
        packer.member(layout, member)?;
    }
    Ok(PackedConstants {
        bytes: packer.bytes,
        missing: packer.missing,
    })
}

struct Packer<'a> {
    layers: &'a [&'a PropertySet],
    bytes: Vec<u8>,
    missing: Vec<String>,
}

/// Whether the packer writes a member of this kind and shape.
fn writes(member: &Member) -> bool {
    let components = (1..=4).contains(&member.cols) && (1..=4).contains(&member.rows);
    match member.kind {
        ScalarKind::F32 => components && (member.matrix.is_some() || member.rows == 1),
        _ => {
            member.rows == 1
                && member.cols == 1
                && member.array.is_none()
                && member.matrix.is_none()
        }
    }
}

impl Packer<'_> {
    fn member(&mut self, layout: &ConstantLayout, member: &Member) -> Result<(), ConstantError> {
        let BindingSource::Property(id) = &member.source else {
            return Err(ConstantError::UnsupportedMember {
                name: layout.name.clone(),
            });
        };
        let name = id.as_str();
        if !writes(member) {
            return Err(ConstantError::UnsupportedMember {
                name: name.to_owned(),
            });
        }
        let size = member.extent();
        if u64::from(member.offset) + size > self.bytes.len() as u64 {
            return Err(ConstantError::OutOfBounds {
                name: name.to_owned(),
                offset: u64::from(member.offset),
                size,
                buffer: self.bytes.len(),
            });
        }
        let Some(value) = lookup(self.layers, name).cloned() else {
            if !self.missing.iter().any(|missing| missing == name) {
                self.missing.push(name.to_owned());
            }
            return Ok(());
        };
        let found = Found {
            name,
            member,
            value,
        };
        match (member.kind, member.matrix) {
            (ScalarKind::F32, None) => self.vector(&found),
            (ScalarKind::F32, Some(matrix)) => self.matrix(&found, matrix),
            _ => self.integer(&found),
        }
    }

    /// Copies `data` to `at`, which `member` has checked to lie inside the buffer.
    fn store(&mut self, at: u64, data: &[u8]) {
        let start = usize::try_from(at).unwrap_or(usize::MAX);
        self.bytes[start..start + data.len()].copy_from_slice(data);
    }

    fn vector(&mut self, found: &Found<'_>) -> Result<(), ConstantError> {
        let cols = usize::from(found.member.cols);
        let elements: Vec<[f32; 4]> = match &found.value {
            PropertyValue::Float(x) if cols == 1 => vec![[*x, 0.0, 0.0, 0.0]],
            PropertyValue::Vector(v) => vec![*v],
            PropertyValue::VectorArray(items) => items.clone(),
            _ => return Err(found.wrong_kind()),
        };
        found.check_length(elements.len())?;
        for (index, element) in (0..).zip(&elements) {
            let data: Vec<u8> = element[..cols]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            self.store(found.element_at(index), &data);
        }
        Ok(())
    }

    fn matrix(&mut self, found: &Found<'_>, layout: MatrixLayout) -> Result<(), ConstantError> {
        let elements: Vec<[f32; 16]> = match &found.value {
            PropertyValue::Matrix(m) => vec![*m],
            PropertyValue::MatrixArray(items) => items.clone(),
            _ => return Err(found.wrong_kind()),
        };
        found.check_length(elements.len())?;
        let (rows, cols) = (
            usize::from(found.member.rows),
            usize::from(found.member.cols),
        );
        let (lines, line_len) = match layout.order {
            MatrixOrder::ColumnMajor => (cols, rows),
            MatrixOrder::RowMajor => (rows, cols),
        };
        for (index, element) in (0..).zip(&elements) {
            for line in 0..lines {
                let data: Vec<u8> = (0..line_len)
                    .map(|k| match layout.order {
                        MatrixOrder::ColumnMajor => element[line * 4 + k],
                        MatrixOrder::RowMajor => element[k * 4 + line],
                    })
                    .flat_map(f32::to_le_bytes)
                    .collect();
                let at = found.element_at(index) + line as u64 * u64::from(layout.stride);
                self.store(at, &data);
            }
        }
        Ok(())
    }

    fn integer(&mut self, found: &Found<'_>) -> Result<(), ConstantError> {
        let bytes = match (found.member.kind, &found.value) {
            (ScalarKind::U32, PropertyValue::Uint(v)) => v.to_le_bytes(),
            (ScalarKind::U32, PropertyValue::Int(v)) => u32::try_from(*v)
                .map_err(|_| found.wrong_kind())?
                .to_le_bytes(),
            (ScalarKind::Bool32, PropertyValue::Bool(v)) => u32::from(*v).to_le_bytes(),
            (ScalarKind::I32, PropertyValue::Int(v))
            | (ScalarKind::Bool32, PropertyValue::Int(v @ (0 | 1))) => v.to_le_bytes(),
            _ => return Err(found.wrong_kind()),
        };
        self.store(u64::from(found.member.offset), &bytes);
        Ok(())
    }
}

/// A member, its property's name and the value found for it.
struct Found<'m> {
    name: &'m str,
    member: &'m Member,
    value: PropertyValue,
}

impl Found<'_> {
    fn wrong_kind(&self) -> ConstantError {
        ConstantError::WrongKind {
            property: self.name.to_owned(),
        }
    }

    fn check_length(&self, given: usize) -> Result<(), ConstantError> {
        let declared = self
            .member
            .array
            .map_or(1, |array| array.count.get() as usize);
        if given > declared {
            return Err(ConstantError::ArrayTooLong {
                property: self.name.to_owned(),
                declared,
                given,
            });
        }
        Ok(())
    }

    /// Where array element `index` starts.
    fn element_at(&self, index: u64) -> u64 {
        let stride = self.member.array.map_or(0, |array| u64::from(array.stride));
        u64::from(self.member.offset) + index * stride
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property(name: &str) -> BindingSource {
        BindingSource::Property(crate::binding::PropertyId::new(name))
    }

    fn vector(name: &str, offset: u32, cols: u8) -> Member {
        Member {
            source: property(name),
            kind: ScalarKind::F32,
            rows: 1,
            cols,
            array: None,
            offset,
            matrix: None,
        }
    }

    fn scalar(name: &str, offset: u32, kind: ScalarKind) -> Member {
        Member {
            kind,
            ..vector(name, offset, 1)
        }
    }

    fn layout(byte_size: u32, members: Vec<Member>) -> ConstantLayout {
        ConstantLayout {
            name: "Material".to_owned(),
            byte_size,
            members,
        }
    }

    fn with(entries: &[(&str, PropertyValue)]) -> PropertySet {
        let mut set = PropertySet::new();
        for (name, value) in entries {
            set.set(name, value.clone());
        }
        set
    }

    fn floats(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    fn words(bytes: &[u8]) -> Vec<u32> {
        bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    #[test]
    fn a_vector_lands_at_its_offset_and_the_rest_stays_zero() {
        let layout = layout(32, vec![vector("tint", 16, 3)]);
        let material = with(&[("tint", PropertyValue::Vector([0.25, 0.5, 0.75, 1.0]))]);
        let packed = pack_constants(&layout, &[&material], 0).unwrap();
        assert_eq!(
            floats(&packed.bytes),
            [0.0, 0.0, 0.0, 0.0, 0.25, 0.5, 0.75, 0.0]
        );
        assert!(packed.missing.is_empty());
    }

    #[test]
    fn the_first_layer_with_a_property_wins_and_a_missing_one_is_reported_once() {
        let layout = layout(32, vec![vector("a", 0, 1), vector("b", 16, 1)]);
        let near = with(&[("a", PropertyValue::Float(1.0))]);
        let far = with(&[("a", PropertyValue::Float(2.0))]);
        let packed = pack_constants(&layout, &[&near, &far], 0).unwrap();
        assert_eq!(floats(&packed.bytes)[0].to_bits(), 1.0_f32.to_bits());
        assert_eq!(packed.missing, ["b".to_owned()]);
    }

    #[test]
    fn the_buffer_is_the_larger_of_the_declared_size_and_the_shaders_need() {
        let layout = layout(20, vec![]);
        assert_eq!(pack_constants(&layout, &[], 0).unwrap().bytes.len(), 20);
        assert_eq!(pack_constants(&layout, &[], 36).unwrap().bytes.len(), 36);
    }

    #[test]
    fn array_elements_sit_a_declared_stride_apart() {
        let mut member = vector("weights", 4, 2);
        member.array = Some(ArrayLayout {
            count: NonZeroU32::new(3).unwrap(),
            stride: 24,
        });
        let layout = layout(76, vec![member]);
        let weights = PropertyValue::VectorArray(vec![[1.0, 2.0, 9.0, 9.0], [3.0, 4.0, 9.0, 9.0]]);
        let packed = pack_constants(&layout, &[&with(&[("weights", weights)])], 0).unwrap();
        let at = |byte: usize| floats(&packed.bytes)[byte / 4];
        assert_eq!(
            [at(4), at(8)].map(f32::to_bits),
            [1.0, 2.0].map(f32::to_bits)
        );
        assert_eq!(
            [at(28), at(32)].map(f32::to_bits),
            [3.0, 4.0].map(f32::to_bits)
        );
        assert_eq!(
            [at(12), at(52)].map(f32::to_bits),
            [0.0, 0.0].map(f32::to_bits)
        );
    }

    #[test]
    fn an_array_longer_than_its_member_is_a_named_error() {
        let layout = layout(16, vec![vector("weights", 0, 4)]);
        let weights = PropertyValue::VectorArray(vec![[0.0; 4]; 2]);
        assert_eq!(
            pack_constants(&layout, &[&with(&[("weights", weights)])], 0).unwrap_err(),
            ConstantError::ArrayTooLong {
                property: "weights".to_owned(),
                declared: 1,
                given: 2
            }
        );
    }

    fn matrix_value() -> PropertyValue {
        let mut values = [0.0; 16];
        for (index, value) in values.iter_mut().enumerate() {
            *value = f32::from(u8::try_from(index).unwrap());
        }
        PropertyValue::Matrix(values)
    }

    fn matrix(order: MatrixOrder, rows: u8, cols: u8, stride: u32) -> Member {
        Member {
            matrix: Some(MatrixLayout { order, stride }),
            rows,
            cols,
            ..vector("m", 0, cols)
        }
    }

    #[test]
    fn a_column_major_matrix_writes_each_column_a_stride_apart() {
        // Column c holds logical values 4c..4c+4, so the full 4 by 4 case copies the property.
        let layout = layout(64, vec![matrix(MatrixOrder::ColumnMajor, 4, 4, 16)]);
        let packed = pack_constants(&layout, &[&with(&[("m", matrix_value())])], 0).unwrap();
        let expected: Vec<f32> = (0..16)
            .map(|i| f32::from(u8::try_from(i).unwrap()))
            .collect();
        assert_eq!(floats(&packed.bytes), expected);
    }

    #[test]
    fn a_row_major_matrix_writes_each_row_from_the_logical_columns() {
        // A 2 by 3 matrix: row r is [m(r,0), m(r,1), m(r,2)] = values r, 4 + r, 8 + r.
        let layout = layout(32, vec![matrix(MatrixOrder::RowMajor, 2, 3, 16)]);
        let packed = pack_constants(&layout, &[&with(&[("m", matrix_value())])], 0).unwrap();
        let all = floats(&packed.bytes);
        assert_eq!(all[0..3], [0.0, 4.0, 8.0]);
        assert_eq!(all[4..7], [1.0, 5.0, 9.0]);
        assert_eq!(all[3].to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn integer_unsigned_and_boolean_members_take_the_values_that_fit() {
        let layout = layout(
            48,
            vec![
                scalar("i", 0, ScalarKind::I32),
                scalar("u", 4, ScalarKind::U32),
                scalar("b", 8, ScalarKind::Bool32),
                scalar("fromint", 12, ScalarKind::U32),
                scalar("flag", 16, ScalarKind::Bool32),
            ],
        );
        let material = with(&[
            ("i", PropertyValue::Int(-3)),
            ("u", PropertyValue::Uint(7)),
            ("b", PropertyValue::Bool(true)),
            ("fromint", PropertyValue::Int(5)),
            ("flag", PropertyValue::Int(0)),
        ]);
        let packed = pack_constants(&layout, &[&material], 0).unwrap();
        assert_eq!(
            words(&packed.bytes)[..5],
            [(-3_i32).cast_unsigned(), 7, 1, 5, 0]
        );
    }

    #[test]
    fn a_value_outside_an_unsigned_or_boolean_member_is_a_wrong_kind() {
        let wrong = |kind, value| {
            let layout = layout(16, vec![scalar("v", 0, kind)]);
            pack_constants(&layout, &[&with(&[("v", value)])], 0).unwrap_err()
        };
        let expected = ConstantError::WrongKind {
            property: "v".to_owned(),
        };
        assert_eq!(wrong(ScalarKind::U32, PropertyValue::Int(-1)), expected);
        assert_eq!(wrong(ScalarKind::Bool32, PropertyValue::Int(2)), expected);
        assert_eq!(wrong(ScalarKind::I32, PropertyValue::Float(1.0)), expected);
        assert_eq!(
            wrong(ScalarKind::F32, PropertyValue::Int(1)),
            expected,
            "a float member does not take an integer"
        );
    }

    #[test]
    fn a_member_past_the_buffer_is_an_error_even_without_a_value() {
        let layout = layout(16, vec![vector("late", 16, 1)]);
        assert!(matches!(
            pack_constants(&layout, &[], 0).unwrap_err(),
            ConstantError::OutOfBounds { .. }
        ));
        let huge = ConstantLayout {
            members: vec![vector("huge", u32::MAX, 4)],
            ..layout
        };
        assert!(matches!(
            pack_constants(&huge, &[], 0).unwrap_err(),
            ConstantError::OutOfBounds { .. }
        ));
    }

    #[test]
    fn shapes_the_packer_does_not_write_are_named() {
        let mut wide_int = scalar("wide", 0, ScalarKind::I32);
        wide_int.cols = 3;
        let layout = layout(16, vec![wide_int]);
        assert_eq!(
            pack_constants(&layout, &[], 0).unwrap_err(),
            ConstantError::UnsupportedMember {
                name: "wide".to_owned()
            }
        );
        let layout = ConstantLayout {
            members: vec![vector("five", 0, 5)],
            ..layout
        };
        assert!(matches!(
            pack_constants(&layout, &[], 0).unwrap_err(),
            ConstantError::UnsupportedMember { .. }
        ));
    }
}
