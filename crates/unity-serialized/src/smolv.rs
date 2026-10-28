//! Decoding of SMOL-V, the compressed SPIR-V encoding Unity stores Vulkan shader modules in,
//! and a structural check of the SPIR-V it produces.
//!
//! The wire format and the opcode table follow smol-v by Aras Pranckevičius
//! (<https://github.com/aras-p/smol-v>), used under its public-domain (Unlicense) option.
//! SMOL-V encoding versions 0 and 1 are supported.

use crate::SerializedError;

/// The first word of a SPIR-V module.
pub const SPIRV_MAGIC: u32 = 0x0723_0203;
const SMOLV_MAGIC: u32 = 0x534d_4f4c;
/// The largest decoded module accepted, in bytes.
const MAX_DECODED: usize = 64 * 1024 * 1024;
/// Magic, version, generator, bound, schema and decoded size.
const HEADER_LEN: usize = 24;

const OP_DECORATE: u32 = 71;
const OP_MEMBER_DECORATE: u32 = 72;
const OP_VECTOR_SHUFFLE: u32 = 79;
/// SMOL-V's own opcode for a vector shuffle with a packed swizzle.
const OP_VECTOR_SHUFFLE_COMPACT: u32 = 13;
const OP_ENTRY_POINT: u32 = 15;
const DECORATION_OFFSET: u32 = 35;

/// Opcodes SMOL-V exchanges so that common ones encode in fewer bits.
const SWAPS: [(u32, u32); 12] = [
    (71, 0),
    (61, 1),
    (62, 2),
    (65, 3),
    (79, 4),
    (72, 7),
    (248, 8),
    (59, 9),
    (133, 10),
    (129, 11),
    (32, 14),
    (127, 15),
];

/// The opcodes encoding version 0 knows; version 1 knows all of [`OPS`].
const VERSION_0_OPS: usize = 331;

/// Per SPIR-V opcode: bit 0 set when it has a result ID, bit 1 when it has a type ID, bit 2
/// when its remaining operands are varints, and bits 3 and up the number of operands stored
/// relative to the result ID.
#[rustfmt::skip]
const OPS: [u8; 367] = [
    0x00, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x04, 0x03, 0x00, 0x01, 0x07, 0x17, 0x04, 0x04,
    0x04, 0x04, 0x03, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x04, 0x03, 0x03, 0x03, 0x03, 0x4b, 0x07, 0x03, 0x03,
    0x03, 0x03, 0x03, 0x4b, 0x03, 0x03, 0x07, 0x03, 0x00, 0x4b, 0x03, 0x07, 0x03, 0x0f, 0x14, 0x00,
    0x00, 0x07, 0x03, 0x03, 0x03, 0x03, 0x03, 0x04, 0x04, 0x01, 0x00, 0x00, 0x03, 0x0f, 0x17, 0x17,
    0x4b, 0x0f, 0x17, 0x0b, 0x03, 0x03, 0x03, 0x17, 0x17, 0x1f, 0x1f, 0x17, 0x17, 0x1f, 0x1f, 0x17,
    0x1f, 0x1f, 0x17, 0x1c, 0x0b, 0x0b, 0x0b, 0x13, 0x0b, 0x13, 0x0b, 0x0b, 0x03, 0x0b, 0x0b, 0x0b,
    0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0f, 0x0b, 0x03, 0x0b, 0x0b,
    0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13,
    0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x03, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b, 0x0b,
    0x0b, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x0b, 0x1b, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13,
    0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13,
    0x03, 0x03, 0x13, 0x13, 0x13, 0x13, 0x13, 0x13, 0x0b, 0x23, 0x1b, 0x1b, 0x0b, 0x0b, 0x03, 0x03,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x03, 0x03,
    0x18, 0x10, 0x03, 0x03, 0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x14, 0x0c, 0x01, 0x08, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x03, 0x03, 0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00,
    0x00, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x00, 0x00, 0x03, 0x03, 0x00, 0x00, 0x03,
    0x03, 0x17, 0x17, 0x1f, 0x1f, 0x17, 0x17, 0x1f, 0x1f, 0x17, 0x1f, 0x1f, 0x0b, 0x00, 0x03, 0x00,
    0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x03, 0x07, 0x14, 0x03, 0x04, 0x04, 0x0f, 0x0f, 0x0f,
    0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f,
    0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f,
];

fn invalid(offset: usize, reason: &'static str) -> SerializedError {
    SerializedError::InvalidShaderModule { offset, reason }
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], SerializedError> {
        let bytes = self
            .data
            .get(self.at..)
            .and_then(<[u8]>::first_chunk::<N>)
            .ok_or(SerializedError::Truncated {
                offset: self.at,
                needed: N,
                len: self.data.len(),
            })?;
        self.at += N;
        Ok(*bytes)
    }

    fn byte(&mut self) -> Result<u8, SerializedError> {
        self.bytes::<1>().map(|[b]| b)
    }

    fn word(&mut self) -> Result<u32, SerializedError> {
        self.bytes().map(u32::from_le_bytes)
    }

    /// A little-endian base-128 integer of at most five bytes.
    fn varint(&mut self) -> Result<u32, SerializedError> {
        let start = self.at;
        let mut value = 0u64;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return u32::try_from(value)
                    .map_err(|_| invalid(start, "a varint exceeds 32 bits"));
            }
        }
        Err(invalid(start, "a varint runs past five bytes"))
    }

    /// A signed difference, zigzag encoded, as a two's complement word.
    fn delta(&mut self) -> Result<u32, SerializedError> {
        let value = self.varint()?;
        Ok((value >> 1) ^ (value & 1).wrapping_neg())
    }
}

/// An instruction's first word: its length in words and its opcode.
fn instruction_word(len: usize, op: u32, at: usize) -> Result<u32, SerializedError> {
    u32::try_from(len)
        .ok()
        .filter(|&len| len <= 0xffff)
        .map(|len| (len << 16) | op)
        .ok_or_else(|| invalid(at, "an instruction is longer than SPIR-V allows"))
}

fn remap(op: u32) -> u32 {
    SWAPS
        .iter()
        .find_map(|&(a, b)| (op == a).then_some(b).or((op == b).then_some(a)))
        .unwrap_or(op)
}

/// An instruction's opcode and length in words.
fn header(r: &mut Reader<'_>) -> Result<(u32, usize), SerializedError> {
    let at = r.at;
    let packed = r.varint()?;
    let op = remap(((packed >> 4) & 0xfff0) | (packed & 15));
    let base = ((packed >> 20) << 4) | ((packed >> 4) & 15);
    let implied = match op {
        OP_VECTOR_SHUFFLE | OP_VECTOR_SHUFFLE_COMPACT => 5,
        OP_DECORATE => 3,
        61 | 65 => 4,
        _ => 1,
    };
    let len = base + implied;
    if len > 0xffff {
        return Err(invalid(at, "an instruction is longer than SPIR-V allows"));
    }
    Ok((op, len as usize))
}

/// The IDs that operands are stored relative to.
#[derive(Default)]
struct State {
    result: u32,
    decorated: u32,
}

/// Decodes the rest of an `OpMemberDecorate` run: SMOL-V stores consecutive member
/// decorations of one structure as one instruction.
fn member_decorations(
    r: &mut Reader<'_>,
    out: &mut Vec<u32>,
    target: u32,
    first_len: usize,
) -> Result<(), SerializedError> {
    let at = r.at;
    let count = r.byte()?;
    if count == 0 {
        return Err(invalid(at, "a member decoration run is empty"));
    }
    let (mut member, mut offset) = (0u32, 0u32);
    for index in 0..count {
        let at = r.at;
        let overflow = || invalid(at, "a member index or offset exceeds 32 bits");
        member = member.checked_add(r.varint()?).ok_or_else(overflow)?;
        let decoration = r.varint()?;
        let operands = match decoration {
            0 | 2..=5 => 0,
            29..=37 => 1,
            _ => r.varint()? as usize,
        };
        let len = operands + 4;
        let word = instruction_word(len, OP_MEMBER_DECORATE, at)?;
        if index == 0 && len != first_len {
            return Err(invalid(
                at,
                "a member decoration has an inconsistent length",
            ));
        }
        if index > 0 {
            out.extend([word, target]);
        }
        out.extend([member, decoration]);
        for operand in 0..operands {
            let mut value = r.varint()?;
            if decoration == DECORATION_OFFSET && operand == 0 {
                offset = offset.checked_add(value).ok_or_else(overflow)?;
                value = offset;
            }
            out.push(value);
        }
    }
    Ok(())
}

/// Decodes one instruction into `out`.
fn instruction(
    r: &mut Reader<'_>,
    out: &mut Vec<u32>,
    state: &mut State,
    known: usize,
) -> Result<(), SerializedError> {
    let (op, len) = header(r)?;
    let compact = op == OP_VECTOR_SHUFFLE_COMPACT;
    let op = if compact { OP_VECTOR_SHUFFLE } else { op };
    let info = OPS
        .get(op as usize)
        .filter(|_| (op as usize) < known)
        .copied()
        .unwrap_or(0);
    out.push(instruction_word(len, op, r.at)?);
    let mut used = 1;
    if info & 2 != 0 {
        out.push(r.varint()?);
        used += 1;
    }
    if info & 1 != 0 {
        state.result = state.result.wrapping_add(r.delta()?);
        out.push(state.result);
        used += 1;
    }
    if op == OP_DECORATE || op == OP_MEMBER_DECORATE {
        state.decorated = state.decorated.wrapping_add(r.delta()?);
        out.push(state.decorated);
        used += 1;
    }
    if op == OP_MEMBER_DECORATE {
        return member_decorations(r, out, state.decorated, len);
    }
    let relative = usize::from(info >> 3).min(len.saturating_sub(used));
    for _ in 0..relative {
        out.push(state.result.wrapping_sub(r.delta()?));
    }
    operands(
        r,
        out,
        len.saturating_sub(used + relative),
        compact.then_some(len),
        info & 4 != 0,
    )
}

/// Decodes an instruction's remaining operands. A compact shuffle of at most nine words
/// (`compact_len`) packs its up to four components, two bits each, into one byte.
fn operands(
    r: &mut Reader<'_>,
    out: &mut Vec<u32>,
    count: usize,
    compact_len: Option<usize>,
    varints: bool,
) -> Result<(), SerializedError> {
    if let Some(len) = compact_len.filter(|&len| len <= 9) {
        let swizzle = r.byte()?;
        out.extend(
            [6, 4, 2, 0]
                .iter()
                .take(len - 5)
                .map(|s| u32::from(swizzle >> s) & 3),
        );
        return Ok(());
    }
    for _ in 0..count {
        out.push(if varints { r.varint()? } else { r.word()? });
    }
    Ok(())
}

/// Decodes a SMOL-V module into SPIR-V words, and checks the result with [`entry_points`].
///
/// # Errors
/// Fails when the data is not SMOL-V of a supported version, is truncated or malformed, or
/// does not decode to exactly its declared size.
pub fn decode(data: &[u8]) -> Result<Vec<u32>, SerializedError> {
    let mut r = Reader { data, at: 0 };
    if data.len() < HEADER_LEN || r.word()? != SMOLV_MAGIC {
        return Err(invalid(0, "the data is not SMOL-V"));
    }
    let version = r.word()?;
    let spirv_version = version & 0xff_ffff;
    let known = match version >> 24 {
        0 => VERSION_0_OPS,
        1 => OPS.len(),
        _ => return Err(invalid(4, "the SMOL-V encoding version is not 0 or 1")),
    };
    if !(0x1_0000..=0x1_0600).contains(&spirv_version) {
        return Err(invalid(4, "the SPIR-V version is not 1.0 to 1.6"));
    }
    let mut out = vec![SPIRV_MAGIC, spirv_version, r.word()?, r.word()?, r.word()?];
    let size = r.word()? as usize;
    if size < 20 || !size.is_multiple_of(4) || size > MAX_DECODED {
        return Err(invalid(20, "the declared SPIR-V size is invalid"));
    }
    let mut state = State::default();
    while r.at < data.len() {
        instruction(&mut r, &mut out, &mut state, known)?;
        if out.len() * 4 > size {
            return Err(invalid(r.at, "the output exceeds the declared size"));
        }
    }
    if out.len() * 4 != size {
        return Err(invalid(
            r.at,
            "the output is shorter than the declared size",
        ));
    }
    entry_points(&out)?;
    Ok(out)
}

/// Checks a SPIR-V module's header and that its instructions exactly fill it, and returns
/// each entry point's execution model (0 vertex, 4 fragment, 5 compute, and so on) in order.
///
/// # Errors
/// Fails when the header is invalid or an instruction is empty or runs past the module.
/// Offsets are in bytes.
pub fn entry_points(words: &[u32]) -> Result<Vec<u32>, SerializedError> {
    if words.len() < 5 || words[0] != SPIRV_MAGIC || words[4] != 0 {
        return Err(invalid(0, "the SPIR-V header is invalid"));
    }
    let mut models = Vec::new();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        let end = at + count;
        if count == 0 || end > words.len() {
            return Err(invalid(
                at * 4,
                "a SPIR-V instruction is empty or runs past the module",
            ));
        }
        if words[at] & 0xffff == OP_ENTRY_POINT {
            let model = words[at + 1..end].first();
            models.push(*model.ok_or_else(|| invalid(at * 4, "an entry point has no model"))?);
        }
        at = end;
    }
    Ok(models)
}
