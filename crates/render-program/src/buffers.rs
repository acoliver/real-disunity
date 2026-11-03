//! A constant buffer's members in the one form the packer reads, whichever parameters
//! described the buffer.
//!
//! Editor-style parameters name a member by an index into the pass's names and give its kind
//! as Unity's `ShaderParamType`, a dimension and a byte offset. A player variant names the
//! member in place and gives its kind, rows, columns, matrix flag, array size and byte offset.
//! Both reduce to a [`Member`].

use worldrunner_unity_serialized::shader_parameters as player;
use worldrunner_unity_serialized::shader_program::{ConstantBuffer, ValueParam};

use crate::constants::ConstantError;

/// How a member or buffer names itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamName {
    /// An index into the pass's names, which the caller supplies when packing.
    Index(i32),
    /// The name itself.
    Named(String),
}

impl ParamName {
    /// The name, looked up in `names` when it is an index.
    ///
    /// # Errors
    /// Fails on an index that `names` does not list.
    pub fn resolve<'a>(&'a self, names: &'a [(String, i32)]) -> Result<&'a str, ConstantError> {
        match self {
            Self::Named(name) => Ok(name),
            Self::Index(name_index) => names
                .iter()
                .find(|(_, index)| index == name_index)
                .map(|(name, _)| name.as_str())
                .ok_or(ConstantError::UnknownName {
                    name_index: *name_index,
                }),
        }
    }
}

/// The scalar type of a member, for the kinds that have been measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Uint,
    /// Any other kind, by the number the parameters gave; the packer does not write it.
    Other(i32),
}

impl Kind {
    /// Unity's `ShaderParamType`, as editor-style parameters store it.
    fn editor(code: i8) -> Self {
        match code {
            0 => Self::Float,
            1 => Self::Int,
            2 => Self::Bool,
            5 => Self::Uint,
            other => Self::Other(i32::from(other)),
        }
    }

    /// A player member's kind, of which only float (0) and int (1) have been measured.
    fn player(code: i32) -> Self {
        match code {
            0 => Self::Float,
            1 => Self::Int,
            other => Self::Other(other),
        }
    }

    /// The number the parameters gave, for an error.
    pub(crate) fn code(self) -> i32 {
        match self {
            Self::Float => 0,
            Self::Int => 1,
            Self::Bool => 2,
            Self::Uint => 5,
            Self::Other(code) => code,
        }
    }
}

/// A scalar, vector or matrix member, or an array of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: ParamName,
    pub kind: Kind,
    /// 1 for a scalar or vector and the row count for a matrix.
    pub rows: i32,
    /// The component count of a scalar or vector, and the column count of a matrix.
    pub columns: i32,
    pub matrix: bool,
    /// The element count of an array member, 0 for a member that is not an array.
    pub array_size: i32,
    /// Byte offset in the buffer.
    pub offset: i32,
}

/// A struct, or an array of structs, in a player's constant buffer. An element's members sit at
/// offsets from the element's own start, and the elements are `size` bytes apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructArray {
    pub name: ParamName,
    /// Where the player says the struct starts. Only 0 has been measured; see
    /// [`BufferLayout::struct_placement`].
    pub index: i32,
    /// The element count, 0 or 1 for a struct that is not an array.
    pub array_size: i32,
    /// Bytes from one element to the next.
    pub size: i32,
    pub members: Vec<Member>,
}

/// A constant buffer: its declared size and the members to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferLayout {
    pub name: ParamName,
    pub size: i32,
    pub members: Vec<Member>,
    /// The structs of a player's buffer.
    pub structs: Vec<StructArray>,
    /// The names of an editor-style buffer's structs, whose layout has not been measured.
    pub editor_structs: Vec<ParamName>,
}

impl BufferLayout {
    /// A buffer of editor-style parameters, whose names are indices.
    #[must_use]
    pub fn from_program(buffer: &ConstantBuffer) -> Self {
        let vectors = buffer
            .vectors
            .iter()
            .map(|param| editor_member(param, false));
        let matrices = buffer
            .matrices
            .iter()
            .map(|param| editor_member(param, true));
        Self {
            name: ParamName::Index(buffer.name_index),
            size: buffer.size,
            members: vectors.chain(matrices).collect(),
            structs: Vec::new(),
            editor_structs: buffer
                .structs
                .iter()
                .map(|s| ParamName::Index(s.name_index))
                .collect(),
        }
    }

    /// A buffer of a player variant, whose names are in place.
    #[must_use]
    pub fn from_variant(buffer: &player::ConstantBuffer) -> Self {
        Self {
            name: ParamName::Named(buffer.name.clone()),
            size: buffer.size,
            members: buffer.members.iter().map(player_member).collect(),
            structs: buffer.structs.iter().map(player_struct).collect(),
            editor_structs: Vec::new(),
        }
    }
}

fn player_struct(layout: &player::StructLayout) -> StructArray {
    StructArray {
        name: ParamName::Named(layout.name.clone()),
        index: layout.index,
        array_size: layout.array_size,
        size: layout.size,
        members: layout.members.iter().map(player_member).collect(),
    }
}

/// An editor-style matrix has `dimension` rows of four floats; a vector has `dimension`
/// components.
fn editor_member(param: &ValueParam, matrix: bool) -> Member {
    let dimension = i32::from(param.dimension);
    Member {
        name: ParamName::Index(param.name_index),
        kind: Kind::editor(param.kind),
        rows: if matrix { dimension } else { 1 },
        columns: if matrix { 4 } else { dimension },
        matrix,
        array_size: param.array_size,
        offset: param.index,
    }
}

fn player_member(member: &player::BufferMember) -> Member {
    Member {
        name: ParamName::Named(member.name.clone()),
        kind: Kind::player(member.kind),
        rows: member.rows,
        columns: member.columns,
        matrix: member.is_matrix,
        array_size: member.array_size,
        offset: member.offset,
    }
}
