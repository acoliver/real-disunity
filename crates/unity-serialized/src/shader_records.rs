//! The sub-program records in a shader platform's decompressed chunks
//! ([`crate::shader::CompiledBlobs::platform_chunks`]), and the stage modules inside a Vulkan
//! program. All of it is little-endian, whatever the serialized file's byte order.

use std::ops::{Range, RangeInclusive};

use crate::{SerializedError, cursor::Cursor, shader_program::PlayerSubProgram};

/// Where one sub-program's record is: a byte range in the chunk its segment index names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramEntry {
    /// The index of the chunk holding the record (Unity's segment).
    pub segment: usize,
    pub range: Range<usize>,
}

const ENTRY_LEN: usize = 12;

fn invalid(offset: usize, reason: &'static str) -> SerializedError {
    SerializedError::InvalidShaderProgram { offset, reason }
}

fn index(value: u32, offset: usize) -> Result<usize, SerializedError> {
    usize::try_from(value).map_err(|_| invalid(offset, "the value does not fit in memory"))
}

/// Reads the sub-program table at the start of the first chunk and checks that every entry
/// lies inside the chunk it names. `chunk_lengths` are the decompressed chunk lengths.
///
/// # Errors
/// Fails when the table is truncated or an entry names a missing chunk or runs past its end.
pub fn program_table(
    first_chunk: &[u8],
    chunk_lengths: &[usize],
) -> Result<Vec<ProgramEntry>, SerializedError> {
    let mut r = Cursor::new(first_chunk, 0, false);
    let count = r.count("sub-program", ENTRY_LEN)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let at = r.position();
        let [offset, len, segment] = [r.u32()?, r.u32()?, r.u32()?];
        let segment = index(segment, at)?;
        let start = index(offset, at)?;
        let end = start
            .checked_add(index(len, at)?)
            .ok_or_else(|| invalid(at, "the entry's range overflows"))?;
        match chunk_lengths.get(segment) {
            None => return Err(invalid(at, "the entry names a chunk that does not exist")),
            Some(&chunk) if end > chunk => {
                return Err(invalid(at, "the entry runs past the end of its chunk"));
            }
            Some(_) => entries.push(ProgramEntry {
                segment,
                range: start..end,
            }),
        }
    }
    Ok(entries)
}

/// The record format version this reader supports, as stored by Unity 2020.2 and later.
pub const RECORD_VERSION: u32 = 202_012_090;

/// Unity's `ShaderGpuProgramType` for a Vulkan SPIR-V program.
pub const SPIRV_PROGRAM: i8 = 25;

/// The `ShaderGpuProgramType`s of programs compiled for a `ShaderCompilerPlatform`, for the
/// platforms whose programs have been checked: Direct3D 11 (4) uses Unity's
/// `DX11VertexSM40` to `DX11DomainSM50` (15 to 22; the player checked uses 15 to 18), and
/// Vulkan (18) uses `SPIRV` (25).
#[must_use]
pub const fn platform_program_types(platform: u32) -> Option<RangeInclusive<i8>> {
    match platform {
        4 => Some(15..=22),
        18 => Some(SPIRV_PROGRAM..=SPIRV_PROGRAM),
        _ => None,
    }
}

/// The type word of a record in the table: Unity's `ShaderGpuProgramType` for a program
/// record. The table also holds parameter records, whose type words are small numbers
/// unrelated to program types; a shader's [`crate::shader_program::PlayerSubProgram`]s say
/// which entries are programs.
///
/// # Errors
/// Fails on a record version other than [`RECORD_VERSION`], or a record shorter than eight bytes.
pub fn record_type(record: &[u8]) -> Result<u32, SerializedError> {
    let mut r = Cursor::new(record, 0, false);
    if r.u32()? != RECORD_VERSION {
        return Err(invalid(0, "the record version is not 202012090"));
    }
    r.u32()
}

/// One compiled sub-program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramRecord {
    /// Unity's `ShaderGpuProgramType`; [`SPIRV_PROGRAM`] is a Vulkan SPIR-V program.
    pub program_type: u32,
    /// Four words after the type, zero in every Vulkan record of the player checked.
    pub stats: [u32; 4],
    /// The keywords stored in the record.
    pub keywords: Vec<String>,
    /// Byte range of the compiled program within the record.
    pub code: Range<usize>,
    /// The bytes after the program, which are not interpreted yet.
    pub rest: Range<usize>,
}

/// An empty string: its length prefix.
const STRING_MIN: usize = 4;

fn byte_array(r: &mut Cursor<'_>) -> Result<Range<usize>, SerializedError> {
    let len = r.count("program code byte", 1)?;
    let start = r.position();
    r.take(len)?;
    let range = start..r.position();
    r.align4()?;
    Ok(range)
}

/// Reads one program record, an entry a [`crate::shader_program::PlayerSubProgram`] names.
///
/// # Errors
/// Fails on a record version other than [`RECORD_VERSION`], or a truncated record.
pub fn read_record(record: &[u8]) -> Result<ProgramRecord, SerializedError> {
    let program_type = record_type(record)?;
    let mut r = Cursor::new(record, 8, false);
    let stats = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
    let count = r.count("program keyword", STRING_MIN)?;
    let mut keywords = Vec::with_capacity(count);
    for _ in 0..count {
        let len = r.count("keyword byte", 1)?;
        let at = r.position();
        let text = std::str::from_utf8(r.take(len)?)
            .map_err(|_| SerializedError::InvalidString { offset: at })?;
        keywords.push(text.to_owned());
        r.align4()?;
    }
    let code = byte_array(&mut r)?;
    Ok(ProgramRecord {
        program_type,
        stats,
        keywords,
        code,
        rest: r.position()..record.len(),
    })
}

/// A Vulkan stage module's encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleEncoding {
    /// SMOL-V, a compressed SPIR-V encoding.
    Smolv,
    SpirV,
}

/// One stage module of a Vulkan program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageModule {
    /// The stage slot, 0 to 5, in the program's wrapper. In the Unity 6000.3.15f1 player
    /// checked, every Vulkan program is a vertex-stage variant holding its vertex module in
    /// slot 0 and its fragment module in slot 1, as their entry points show.
    pub slot: usize,
    pub encoding: ModuleEncoding,
    /// Byte range of the module within the program code.
    pub range: Range<usize>,
}

const STAGE_SLOTS: usize = 6;
/// A leading word, then an offset and size for each stage slot.
const WRAPPER_LEN: usize = 4 + STAGE_SLOTS * 8;
const SMOLV_MAGIC: u32 = 0x534d_4f4c;
const SPIRV_MAGIC: u32 = 0x0723_0203;

/// Finds the stage modules in a Vulkan program's code. Empty slots are skipped.
///
/// # Errors
/// Fails when the wrapper is truncated, a slot's module lies outside the code or overlaps the
/// wrapper, or a module is neither SMOL-V nor SPIR-V.
pub fn vulkan_stages(code: &[u8]) -> Result<Vec<StageModule>, SerializedError> {
    if code.len() < WRAPPER_LEN {
        return Err(invalid(
            0,
            "the Vulkan program is shorter than its stage table",
        ));
    }
    let mut r = Cursor::new(code, 4, false);
    let mut stages = Vec::new();
    for slot in 0..STAGE_SLOTS {
        let at = r.position();
        let [offset, size] = [r.u32()?, r.u32()?];
        if size == 0 {
            continue;
        }
        let start = index(offset, at)?;
        let range = start..start.saturating_add(index(size, at)?);
        if start < WRAPPER_LEN || range.end > code.len() {
            return Err(invalid(at, "the stage module lies outside the program"));
        }
        // A module too short for a magic number is rejected below rather than classified
        // by bytes beyond it.
        let magic = code[range.clone()]
            .first_chunk::<4>()
            .map(|bytes| u32::from_le_bytes(*bytes));
        let encoding = match magic {
            Some(SMOLV_MAGIC) => ModuleEncoding::Smolv,
            Some(SPIRV_MAGIC) => ModuleEncoding::SpirV,
            _ => {
                return Err(invalid(
                    start,
                    "the stage module is neither SMOL-V nor SPIR-V",
                ));
            }
        };
        stages.push(StageModule {
            slot,
            encoding,
            range,
        });
    }
    Ok(stages)
}

/// A variant's program record for one platform, with the caller's label for the variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformProgram<L> {
    pub label: L,
    pub entry: ProgramEntry,
    pub record: ProgramRecord,
}

impl<L> PlatformProgram<L> {
    /// The program's code, from the chunks the record was read from.
    #[must_use]
    pub fn code<'c>(&self, chunks: &'c [Vec<u8>]) -> Option<&'c [u8]> {
        chunks
            .get(self.entry.segment)?
            .get(self.entry.range.clone())?
            .get(self.record.code.clone())
    }
}

/// Reads the program record of each variant compiled for a platform, in variant order.
/// `types` are the platform's program types ([`platform_program_types`]); variants of other
/// types belong to other platforms and are skipped. A platform with no chunks has no table,
/// so it may have no variants.
///
/// # Errors
/// Fails when the table is malformed, or a variant of the platform names a missing entry, a
/// record of another type, or a malformed record.
pub fn platform_programs<'v, L>(
    variants: impl IntoIterator<Item = (L, &'v PlayerSubProgram)>,
    types: &RangeInclusive<i8>,
    chunks: &[Vec<u8>],
) -> Result<Vec<PlatformProgram<L>>, SerializedError> {
    let lengths: Vec<usize> = chunks.iter().map(Vec::len).collect();
    let entries = match chunks.first() {
        Some(first) => program_table(first, &lengths)?,
        None => Vec::new(),
    };
    let mut programs = Vec::new();
    for (label, variant) in variants {
        if !types.contains(&variant.gpu_program_type) {
            continue;
        }
        let entry = usize::try_from(variant.blob_index)
            .ok()
            .and_then(|index| entries.get(index))
            .ok_or_else(|| invalid(0, "a variant names a blob entry that does not exist"))?;
        let record = chunks
            .get(entry.segment)
            .and_then(|chunk| chunk.get(entry.range.clone()))
            .ok_or_else(|| invalid(0, "a variant's entry lies outside the chunks"))?;
        if i64::from(record_type(record)?) != i64::from(variant.gpu_program_type) {
            return Err(invalid(
                0,
                "a variant's record has a different program type",
            ));
        }
        programs.push(PlatformProgram {
            label,
            entry: entry.clone(),
            record: read_record(record)?,
        });
    }
    Ok(programs)
}
