//! Type trees: the field layouts a serialized file may carry for its types. Player builds
//! usually strip them; editor files and asset bundles keep them.

use crate::{SerializedError, cursor::Cursor};

/// Bytes per node in the type tree blob for format versions 19 and later.
const NODE_LEN: usize = 32;
/// String offsets with this bit set refer to Unity's built-in string table.
const COMMON_STRING: u32 = 1 << 31;

/// A node name or type name. Unity stores common names, such as `int` or `m_Name`, as
/// offsets into a string table built into the engine, which the reader does not carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeString {
    Local(String),
    Common(u32),
}

/// One field of a type tree, in depth-first order with its depth in `level`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeTreeNode {
    pub version: u16,
    pub level: u8,
    pub type_flags: u8,
    pub type_name: NodeString,
    pub name: NodeString,
    pub byte_size: i32,
    pub index: i32,
    pub meta_flags: u32,
    pub ref_type_hash: u64,
}

impl TypeTreeNode {
    /// Whether the field is aligned to four bytes after it is read.
    #[must_use]
    pub fn aligns(&self) -> bool {
        const ALIGN_BYTES: u32 = 0x4000;
        self.meta_flags & ALIGN_BYTES != 0
    }
}

struct RawNode {
    node: TypeTreeNode,
    type_offset: u32,
    name_offset: u32,
}

pub(crate) fn read(cursor: &mut Cursor<'_>) -> Result<Vec<TypeTreeNode>, SerializedError> {
    let nodes = cursor.count("type tree node", NODE_LEN)?;
    let strings_len = cursor.count("type tree string byte", 1)?;
    let raw = (0..nodes)
        .map(|_| read_node(cursor))
        .collect::<Result<Vec<_>, _>>()?;
    let strings_at = cursor.position();
    let strings = cursor.take(strings_len)?;
    raw.into_iter()
        .map(|raw| {
            let mut node = raw.node;
            node.type_name = node_string(strings, strings_at, raw.type_offset)?;
            node.name = node_string(strings, strings_at, raw.name_offset)?;
            Ok(node)
        })
        .collect()
}

fn read_node(cursor: &mut Cursor<'_>) -> Result<RawNode, SerializedError> {
    let version = cursor.u16()?;
    let level = cursor.u8()?;
    let type_flags = cursor.u8()?;
    let type_offset = cursor.u32()?;
    let name_offset = cursor.u32()?;
    let byte_size = cursor.i32()?;
    let index = cursor.i32()?;
    let meta_flags = cursor.u32()?;
    let ref_type_hash = cursor.u64()?;
    Ok(RawNode {
        node: TypeTreeNode {
            version,
            level,
            type_flags,
            type_name: NodeString::Common(0),
            name: NodeString::Common(0),
            byte_size,
            index,
            meta_flags,
            ref_type_hash,
        },
        type_offset,
        name_offset,
    })
}

fn node_string(
    strings: &[u8],
    strings_at: usize,
    offset: u32,
) -> Result<NodeString, SerializedError> {
    if offset & COMMON_STRING != 0 {
        return Ok(NodeString::Common(offset & !COMMON_STRING));
    }
    let start = usize::try_from(offset).unwrap_or(usize::MAX);
    let mut cursor = Cursor::new(strings, start, false);
    if start >= strings.len() {
        return Err(SerializedError::UnterminatedString {
            offset: strings_at.saturating_add(start),
        });
    }
    cursor
        .string()
        .map(NodeString::Local)
        .map_err(|error| match error {
            SerializedError::UnterminatedString { offset } => SerializedError::UnterminatedString {
                offset: strings_at + offset,
            },
            SerializedError::InvalidString { offset } => SerializedError::InvalidString {
                offset: strings_at + offset,
            },
            other => other,
        })
}
