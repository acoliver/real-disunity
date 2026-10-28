use worldrunner_unity_serialized::{
    SerializedError,
    smolv::{SPIRV_MAGIC, decode, entry_points},
};

/// A module covering each SMOL-V rule, in order:
/// - `OpCapability Shader`: an operand stored as a varint;
/// - `OpEntryPoint Fragment %4 "m"`: three varint operands;
/// - `OpTypeFloat %2 32`: a result ID stored as a delta;
/// - `OpConstant %2 %3 1.5`: a type ID, a result delta and a literal word;
/// - `OpDecorate %3 Location 7`: a swapped opcode, an implied length and a decorated-ID delta;
/// - an `OpMemberDecorate` run of four: `Offset`s 0, 16 and 48 stored as deltas from the
///   previous one;
/// - a compact `OpVectorShuffle %2 %5 %3 %3 3 2`: operands relative to the result ID and a
///   packed swizzle;
/// - an `OpVectorShuffle` of five components: a swapped opcode and varint components.
///
/// The upstream C++ decoder decodes these bytes to exactly [`SPIRV`].
#[rustfmt::skip]
const SMOLV: [u8; 78] = [
    0x4c, 0x4f, 0x4d, 0x53, 0x00, 0x00, 0x01, 0x01, 0x07, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0xe8, 0x00, 0x00, 0x00, 0x91, 0x02, 0x01, 0xbf, 0x0e, 0x04,
    0x04, 0x6d, 0xa6, 0x02, 0x04, 0x20, 0xbb, 0x04, 0x02, 0x02, 0x00, 0x00, 0xc0, 0x3f, 0x10,
    0x06, 0x1e, 0x07, 0x47, 0x00, 0x04, 0x00, 0x23, 0x00, 0x01, 0x23, 0x10, 0x01, 0x23, 0x20,
    0x00, 0x05, 0x2d, 0x02, 0x04, 0x04, 0x04, 0xe0, 0x54, 0x02, 0x02, 0x02, 0x06, 0x00, 0x01,
    0x02, 0x03, 0x00,
];

#[rustfmt::skip]
const SPIRV: [u32; 58] = [
    0x0723_0203, 0x0001_0000, 0x0000_0007, 0x0000_0009, 0x0000_0000, 0x0002_0011, 0x0000_0001,
    0x0004_000f, 0x0000_0004, 0x0000_0004, 0x0000_006d, 0x0003_0016, 0x0000_0002, 0x0000_0020,
    0x0004_002b, 0x0000_0002, 0x0000_0003, 0x3fc0_0000, 0x0004_0047, 0x0000_0003, 0x0000_001e,
    0x0000_0007, 0x0005_0048, 0x0000_0003, 0x0000_0000, 0x0000_0023, 0x0000_0000, 0x0005_0048,
    0x0000_0003, 0x0000_0001, 0x0000_0023, 0x0000_0010, 0x0005_0048, 0x0000_0003, 0x0000_0002,
    0x0000_0023, 0x0000_0030, 0x0004_0048, 0x0000_0003, 0x0000_0002, 0x0000_0005, 0x0007_004f,
    0x0000_0002, 0x0000_0005, 0x0000_0003, 0x0000_0003, 0x0000_0003, 0x0000_0002, 0x000a_004f,
    0x0000_0002, 0x0000_0006, 0x0000_0005, 0x0000_0003, 0x0000_0000, 0x0000_0001, 0x0000_0002,
    0x0000_0003, 0x0000_0000,
];

fn reason(error: SerializedError) -> &'static str {
    match error {
        SerializedError::InvalidShaderModule { reason, .. } => reason,
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn decodes_each_instruction_form() {
    assert_eq!(decode(&SMOLV).unwrap(), SPIRV);
    assert_eq!(entry_points(&SPIRV).unwrap(), [4]);
}

#[test]
fn every_truncated_module_fails() {
    for end in 0..SMOLV.len() {
        assert!(decode(&SMOLV[..end]).is_err(), "prefix {end}");
    }
}

#[test]
fn rejects_headers_it_does_not_support() {
    let with = |at: usize, value: u32| {
        let mut bytes = SMOLV;
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        reason(decode(&bytes).unwrap_err())
    };
    assert_eq!(with(0, SPIRV_MAGIC), "the data is not SMOL-V");
    assert_eq!(
        with(4, 0x0201_0000),
        "the SMOL-V encoding version is not 0 or 1"
    );
    assert_eq!(with(4, 0x0101_0700), "the SPIR-V version is not 1.0 to 1.6");
    assert_eq!(with(20, 234), "the declared SPIR-V size is invalid");
    assert_eq!(with(20, 228), "the output exceeds the declared size");
    assert_eq!(
        with(20, 236),
        "the output is shorter than the declared size"
    );
}

#[test]
fn rejects_overlong_varints_and_empty_member_runs() {
    let mut long = SMOLV[..24].to_vec();
    long.extend([0xff; 5]);
    assert_eq!(
        reason(decode(&long).unwrap_err()),
        "a varint runs past five bytes"
    );
    let mut wide = SMOLV[..24].to_vec();
    wide.extend([0xff, 0xff, 0xff, 0xff, 0x7f]);
    assert_eq!(
        reason(decode(&wide).unwrap_err()),
        "a varint exceeds 32 bits"
    );
    let mut empty = SMOLV;
    // The member decoration run's count byte.
    assert_eq!(empty[50], 4);
    empty[50] = 0;
    assert_eq!(
        reason(decode(&empty).unwrap_err()),
        "a member decoration run is empty"
    );
}

#[test]
fn checks_spirv_structure() {
    assert_eq!(
        reason(entry_points(&SPIRV[..4]).unwrap_err()),
        "the SPIR-V header is invalid"
    );
    let mut empty = SPIRV;
    empty[5] = 0x0000_0011;
    assert_eq!(
        reason(entry_points(&empty).unwrap_err()),
        "a SPIR-V instruction is empty or runs past the module"
    );
    assert_eq!(
        reason(entry_points(&SPIRV[..57]).unwrap_err()),
        "a SPIR-V instruction is empty or runs past the module"
    );
}
