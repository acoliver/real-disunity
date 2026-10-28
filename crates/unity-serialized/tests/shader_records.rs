use worldrunner_unity_serialized::{
    SerializedError,
    shader_records::{
        ModuleEncoding, ProgramEntry, RECORD_VERSION, StageModule, platform_program_types,
        program_table, read_record, record_type, vulkan_stages,
    },
};

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn reason(error: SerializedError) -> &'static str {
    match error {
        SerializedError::InvalidShaderProgram { reason, .. } => reason,
        other => panic!("unexpected error {other}"),
    }
}

#[test]
fn reads_the_program_table_and_checks_each_entry_fits_its_chunk() {
    let table = words(&[2, 8, 20, 1, 0, 4, 0]);
    let entries = program_table(&table, &[40, 28]).unwrap();
    assert_eq!(
        entries,
        vec![
            ProgramEntry {
                segment: 1,
                range: 8..28,
            },
            ProgramEntry {
                segment: 0,
                range: 0..4,
            },
        ]
    );
    let missing = words(&[1, 0, 4, 2]);
    assert_eq!(
        reason(program_table(&missing, &[40, 28]).unwrap_err()),
        "the entry names a chunk that does not exist"
    );
    let past_end = words(&[1, 8, 21, 1]);
    assert_eq!(
        reason(program_table(&past_end, &[40, 28]).unwrap_err()),
        "the entry runs past the end of its chunk"
    );
    assert!(matches!(
        program_table(&words(&[2, 0, 4, 0]), &[40]).unwrap_err(),
        SerializedError::InvalidCount { .. }
    ));
}

const CODE: [u8; 5] = [0xc0, 0xc1, 0xc2, 0xc3, 0xc4];

/// A record with two keywords that need padding, five bytes of code and two trailing bytes.
fn record() -> Vec<u8> {
    let mut bytes = words(&[RECORD_VERSION, 25, 1, 2, 3, 4, 2, 1]);
    bytes.extend(b"A\0\0\0");
    bytes.extend(words(&[4]));
    bytes.extend(b"BCDE");
    bytes.extend(words(&[5]));
    bytes.extend(CODE);
    bytes.extend([0, 0, 0, 9, 9]);
    bytes
}

#[test]
fn reads_a_program_record() {
    let bytes = record();
    let program = read_record(&bytes).unwrap();
    assert_eq!(program.program_type, 25);
    assert_eq!(program.stats, [1, 2, 3, 4]);
    assert_eq!(program.keywords, ["A", "BCDE"]);
    assert_eq!(bytes[program.code.clone()], CODE);
    assert_eq!(program.rest, bytes.len() - 2..bytes.len());
    assert_eq!(record_type(&bytes).unwrap(), 25);
}

#[test]
fn rejects_other_versions_truncation_and_text_that_is_not_utf8() {
    let mut other = record();
    other[0] ^= 1;
    assert_eq!(
        reason(read_record(&other).unwrap_err()),
        "the record version is not 202012090"
    );
    assert_eq!(
        reason(record_type(&other).unwrap_err()),
        "the record version is not 202012090"
    );
    let bytes = record();
    // The code's padding ends two bytes before the end.
    for end in 0..bytes.len() - 2 {
        let error = read_record(&bytes[..end]).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
    let mut text = record();
    text[40] = 0xff;
    assert_eq!(
        read_record(&text).unwrap_err(),
        SerializedError::InvalidString { offset: 40 }
    );
}

/// Vulkan program code: the stage table, a SMOL-V module in slot 1 and SPIR-V in slot 4.
fn vulkan_code(slot_one_offset: u32, magic: u32) -> Vec<u8> {
    let mut slots = [0u32; 13];
    slots[3] = slot_one_offset;
    slots[4] = 8;
    slots[9] = 60;
    slots[10] = 12;
    let mut code = words(&slots);
    code.extend(words(&[magic, 7, 0x0723_0203, 1, 2]));
    code
}

#[test]
fn finds_the_stage_modules_of_a_vulkan_program() {
    let stages = vulkan_stages(&vulkan_code(52, 0x534d_4f4c)).unwrap();
    assert_eq!(
        stages,
        vec![
            StageModule {
                slot: 1,
                encoding: ModuleEncoding::Smolv,
                range: 52..60,
            },
            StageModule {
                slot: 4,
                encoding: ModuleEncoding::SpirV,
                range: 60..72,
            },
        ]
    );
}

#[test]
fn rejects_stage_modules_outside_the_program_or_of_unknown_encoding() {
    assert_eq!(
        reason(vulkan_stages(&vulkan_code(48, 0x534d_4f4c)).unwrap_err()),
        "the stage module lies outside the program"
    );
    assert_eq!(
        reason(vulkan_stages(&vulkan_code(68, 0x534d_4f4c)).unwrap_err()),
        "the stage module lies outside the program"
    );
    assert_eq!(
        reason(vulkan_stages(&vulkan_code(52, 0x1234_5678)).unwrap_err()),
        "the stage module is neither SMOL-V nor SPIR-V"
    );
    assert_eq!(
        reason(vulkan_stages(&[0; 51]).unwrap_err()),
        "the Vulkan program is shorter than its stage table"
    );
}

#[test]
fn knows_the_program_types_of_checked_platforms_only() {
    assert_eq!(platform_program_types(4), Some(15..=22));
    assert_eq!(platform_program_types(18), Some(25..=25));
    assert_eq!(platform_program_types(14), None);
}
