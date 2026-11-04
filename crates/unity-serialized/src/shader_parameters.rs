//! The per-variant data a Unity 6 player stores beside each compiled program: the parameters
//! the variant reads, in a blob entry of its own, and its vertex input table, in the tail of
//! its program record.
//!
//! Both are little-endian, whatever the serialized file's byte order, and 4-byte aligned
//! within their entry.

use std::ops::RangeInclusive;

use crate::{
    SerializedError,
    cursor::Cursor,
    shader_program::{BindChannels, PlayerSubProgram, Program},
    shader_records::{RECORD_VERSION, invalid, program_table},
};

/// Where a resource is bound: the packed word the parameters store. The low 16 bits are the
/// binding index, the next ten the descriptor set and the six above them the stages that use
/// the resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescriptorSlot {
    pub binding: u16,
    pub set: u16,
    /// Bit 0 is the vertex stage and bit 1 the fragment stage, the two that have been seen.
    pub stages: u8,
}

impl DescriptorSlot {
    pub const VERTEX: u8 = 1;
    pub const FRAGMENT: u8 = 2;

    /// Splits a packed slot word.
    #[must_use]
    pub const fn unpack(word: u32) -> Self {
        let [b0, b1, b2, b3] = word.to_le_bytes();
        Self {
            binding: u16::from_le_bytes([b0, b1]),
            set: u16::from_le_bytes([b2, b3 & 0x03]),
            stages: b3 >> 2,
        }
    }
}

/// A scalar, vector or matrix member of a constant buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferMember {
    pub name: String,
    /// The scalar type: 0 for float and 1 for int in the player checked.
    pub kind: i32,
    /// 1 for a scalar or vector, and the matrix's row count for a matrix.
    pub rows: i32,
    pub columns: i32,
    pub is_matrix: bool,
    /// The element count of an array member, 0 for a member that is not an array.
    pub array_size: i32,
    /// Byte offset in the constant buffer.
    pub offset: i32,
}

/// A struct inside a constant buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructLayout {
    pub name: String,
    pub index: i32,
    pub array_size: i32,
    pub size: i32,
    pub members: Vec<BufferMember>,
}

/// A constant buffer's layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantBuffer {
    /// Empty for the first buffer of every variant in the player checked, which has no
    /// binding.
    pub name: String,
    pub size: i32,
    pub members: Vec<BufferMember>,
    pub structs: Vec<StructLayout>,
}

/// What a resource binding binds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    /// A sampled texture. `sampler` is the separate sampler it is read with, if it has one.
    /// `dimension` is a code in which 4, 6, 8 and 10 are 2D, 3D, cube and 2D array textures.
    Texture {
        sampler: Option<DescriptorSlot>,
        dimension: i32,
    },
    /// A constant buffer, laid out by the [`ConstantBuffer`] of the same name.
    ConstantBuffer,
    /// A buffer block.
    Buffer,
    /// A storage image.
    StorageImage,
    /// A sampler with no name; `state` is Unity's packed sampler state.
    Sampler { state: u32 },
}

/// One resource a variant binds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceBinding {
    pub name: String,
    pub slot: DescriptorSlot,
    pub kind: ResourceKind,
}

/// The parameters of one variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantParameters {
    pub constant_buffers: Vec<ConstantBuffer>,
    pub bindings: Vec<ResourceBinding>,
}

/// Fewest bytes of a member: its name's length word and six more words.
const MEMBER_LEN: usize = 7 * 4;
/// Fewest bytes of a struct: its name's length word, three words and an empty member list.
const STRUCT_LEN: usize = 5 * 4;
/// Fewest bytes of a constant buffer: its name's length word, its size and two empty lists.
const BUFFER_LEN: usize = 4 * 4;
/// Fewest bytes of a binding: its name's length word, its kind and two more words.
const BINDING_LEN: usize = 4 * 4;

fn flag(r: &mut Cursor<'_>) -> Result<bool, SerializedError> {
    let at = r.position();
    match r.u32()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid(at, "a buffer member's matrix flag is not 0 or 1")),
    }
}

fn read_member(r: &mut Cursor<'_>) -> Result<BufferMember, SerializedError> {
    Ok(BufferMember {
        name: r.sized_string()?,
        kind: r.i32()?,
        rows: r.i32()?,
        columns: r.i32()?,
        is_matrix: flag(r)?,
        array_size: r.i32()?,
        offset: r.i32()?,
    })
}

fn read_members(r: &mut Cursor<'_>) -> Result<Vec<BufferMember>, SerializedError> {
    let count = r.count("buffer member", MEMBER_LEN)?;
    (0..count).map(|_| read_member(r)).collect()
}

fn read_struct(r: &mut Cursor<'_>) -> Result<StructLayout, SerializedError> {
    Ok(StructLayout {
        name: r.sized_string()?,
        index: r.i32()?,
        array_size: r.i32()?,
        size: r.i32()?,
        members: read_members(r)?,
    })
}

fn read_buffer(r: &mut Cursor<'_>) -> Result<ConstantBuffer, SerializedError> {
    let name = r.sized_string()?;
    let size = r.i32()?;
    let members = read_members(r)?;
    let count = r.count("buffer struct", STRUCT_LEN)?;
    let structs = (0..count)
        .map(|_| read_struct(r))
        .collect::<Result<_, _>>()?;
    Ok(ConstantBuffer {
        name,
        size,
        members,
        structs,
    })
}

/// A sampler word that names no sampler.
const NO_SAMPLER: u32 = u32::MAX;

/// Reads the word after a binding's slot, which a kind with no other data fixes.
fn last_word(r: &mut Cursor<'_>, expected: u32) -> Result<(), SerializedError> {
    let at = r.position();
    if r.u32()? == expected {
        Ok(())
    } else {
        Err(invalid(
            at,
            "a resource binding's last word is not the one its kind has",
        ))
    }
}

fn read_binding(r: &mut Cursor<'_>) -> Result<ResourceBinding, SerializedError> {
    let name = r.sized_string()?;
    let at = r.position();
    let tag = r.u32()?;
    let slot = DescriptorSlot::unpack(r.u32()?);
    let kind = match tag {
        0 => {
            let sampler = match r.u32()? {
                NO_SAMPLER => None,
                word => Some(DescriptorSlot::unpack(word)),
            };
            ResourceKind::Texture {
                sampler,
                dimension: r.i32()?,
            }
        }
        1 => {
            last_word(r, 0)?;
            ResourceKind::ConstantBuffer
        }
        2 => {
            last_word(r, 1)?;
            ResourceKind::Buffer
        }
        3 => {
            last_word(r, 1)?;
            ResourceKind::StorageImage
        }
        4 => ResourceKind::Sampler { state: r.u32()? },
        _ => return Err(invalid(at, "a resource binding has an unknown kind")),
    };
    Ok(ResourceBinding { name, slot, kind })
}

/// Reads one variant's parameters from its blob entry, which must be consumed exactly.
///
/// The entry is the record version word, then the variant's constant buffers and its resource
/// bindings, each a count and that many elements. Names are stored in place.
///
/// # Errors
/// Fails on a version other than [`RECORD_VERSION`], truncated or oversized data, a name that
/// is not UTF-8, a binding of an unknown kind, or bytes after the last binding.
pub fn read_parameters(blob: &[u8]) -> Result<VariantParameters, SerializedError> {
    let mut r = Cursor::new(blob, 0, false);
    if r.u32()? != RECORD_VERSION {
        return Err(invalid(0, "the parameters version is not 202012090"));
    }
    let count = r.count("constant buffer", BUFFER_LEN)?;
    let constant_buffers = (0..count)
        .map(|_| read_buffer(&mut r))
        .collect::<Result<_, _>>()?;
    let count = r.count("resource binding", BINDING_LEN)?;
    let bindings = (0..count)
        .map(|_| read_binding(&mut r))
        .collect::<Result<_, _>>()?;
    if r.remaining() != 0 {
        return Err(invalid(
            r.position(),
            "the parameters have bytes after the last binding",
        ));
    }
    Ok(VariantParameters {
        constant_buffers,
        bindings,
    })
}

/// Reads a variant's vertex input table from the tail of its program record
/// ([`crate::shader_records::ProgramRecord::rest`]), which must be consumed exactly.
///
/// The tail is a mask of the source channels the variant reads (`source_map`), a count and that
/// many pairs of words, each a source channel and the shader input it feeds. In the player
/// checked the mask has one bit set per pair, the bit of the pair's source channel.
///
/// # Errors
/// Fails on truncated data, a channel that does not fit in a byte, or extra bytes.
pub fn read_bind_channels(tail: &[u8]) -> Result<BindChannels, SerializedError> {
    let mut r = Cursor::new(tail, 0, false);
    let source_map = r.i32()?;
    let count = r.count("vertex input channel", 8)?;
    let mut channels = Vec::with_capacity(count);
    for _ in 0..count {
        let at = r.position();
        let mut pair = [0i8; 2];
        for channel in &mut pair {
            *channel = i8::try_from(r.i32()?)
                .map_err(|_| invalid(at, "a vertex input channel does not fit in a byte"))?;
        }
        channels.push(pair);
    }
    if r.remaining() != 0 {
        return Err(invalid(
            r.position(),
            "the vertex input table has extra bytes",
        ));
    }
    Ok(BindChannels {
        channels,
        source_map,
    })
}

/// Reads the parameters of each variant of `program` compiled for a platform, in variant
/// order. `types` are the platform's program types
/// ([`crate::shader_records::platform_program_types`]); variants of other types belong to
/// other platforms and are skipped. `chunks` are the platform's decompressed chunks.
///
/// # Errors
/// Fails when the program's parameter lists do not pair with its variants, the platform's
/// table is malformed, a variant names an entry that does not exist, or an entry is malformed.
pub fn platform_parameters<'p>(
    program: &'p Program,
    types: &RangeInclusive<i8>,
    chunks: &[Vec<u8>],
) -> Result<Vec<(&'p PlayerSubProgram, VariantParameters)>, SerializedError> {
    let pairs = program.variant_blobs()?;
    let lengths: Vec<usize> = chunks.iter().map(Vec::len).collect();
    let entries = match chunks.first() {
        Some(first) => program_table(first, &lengths)?,
        None => Vec::new(),
    };
    let mut found = Vec::new();
    for (variant, index) in pairs {
        if !types.contains(&variant.gpu_program_type) {
            continue;
        }
        let blob = usize::try_from(index)
            .ok()
            .and_then(|index| entries.get(index))
            .and_then(|entry| entry.bytes(chunks))
            .ok_or_else(|| invalid(0, "a variant names a parameter entry that does not exist"))?;
        found.push((variant, read_parameters(blob)?));
    }
    Ok(found)
}
