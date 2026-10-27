//! Type trees: the field layouts a serialized file may carry for its types. Player builds
//! usually strip them; editor files and asset bundles keep them.

use std::{collections::BTreeMap, sync::Arc};

use crate::{SerializedError, cursor::Cursor};

/// Bytes per node in the type tree blob for format versions 19 and later.
const NODE_LEN: usize = 32;
/// String offsets with this bit set refer to Unity's built-in string table.
const COMMON_STRING: u32 = 1 << 31;

/// A node name or type name. Unity stores common names, such as `int` or `m_Name`, as
/// offsets into a string table built into the engine, which the reader does not carry.
/// Local names are shared between the nodes that use them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeString {
    Local(Arc<str>),
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
    let mut strings = Strings {
        at: cursor.position(),
        bytes: cursor.take(strings_len)?,
        decoded: BTreeMap::new(),
    };
    raw.into_iter()
        .map(|raw| {
            let mut node = raw.node;
            node.type_name = strings.get(raw.type_offset)?;
            node.name = strings.get(raw.name_offset)?;
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

/// The string buffer of one type tree. Each string is decoded once and shared, so the work
/// and memory spent are bounded by the buffer's size however many nodes name it.
struct Strings<'a> {
    at: usize,
    bytes: &'a [u8],
    decoded: BTreeMap<usize, Arc<str>>,
}

impl Strings<'_> {
    fn get(&mut self, offset: u32) -> Result<NodeString, SerializedError> {
        if offset & COMMON_STRING != 0 {
            return Ok(NodeString::Common(offset & !COMMON_STRING));
        }
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if let Some(text) = self.decoded.get(&start) {
            return Ok(NodeString::Local(Arc::clone(text)));
        }
        // Unity points names at the start of a string. Rejecting offsets inside one keeps
        // the decoded strings disjoint.
        let at_start = start == 0 || self.bytes.get(start.wrapping_sub(1)) == Some(&0);
        if start >= self.bytes.len() || !at_start {
            return Err(SerializedError::TypeTreeString {
                offset,
                len: self.bytes.len(),
            });
        }
        let mut cursor = Cursor::new(self.bytes, start, false);
        let text: Arc<str> = cursor.string().map_err(|e| self.relocate(e))?.into();
        self.decoded.insert(start, Arc::clone(&text));
        Ok(NodeString::Local(text))
    }

    /// Reports a string error at its offset in the file rather than in the buffer.
    fn relocate(&self, error: SerializedError) -> SerializedError {
        match error {
            SerializedError::UnterminatedString { offset } => SerializedError::UnterminatedString {
                offset: self.at + offset,
            },
            SerializedError::InvalidString { offset } => SerializedError::InvalidString {
                offset: self.at + offset,
            },
            other => other,
        }
    }
}
