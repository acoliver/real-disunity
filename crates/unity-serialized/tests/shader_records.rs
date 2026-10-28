use worldrunner_unity_serialized::{
    SerializedError,
    shader_program::PlayerSubProgram,
    shader_records::{
        ModuleEncoding, ProgramEntry, RECORD_VERSION, SPIRV_PROGRAM, StageModule,
        platform_program_types, platform_programs, program_table, read_record, record_type,
        vulkan_stages,
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
    // A three-byte module followed by SMOL-V magic is not classified by bytes beyond it.
    let mut short = words(&[0, 52, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    short.extend(words(&[0x534d_4f4c, 0x534d_4f4c]));
    assert_eq!(
        reason(vulkan_stages(&short).unwrap_err()),
        "the stage module is neither SMOL-V nor SPIR-V"
    );
}

#[test]
fn knows_the_program_types_of_checked_platforms_only() {
    assert_eq!(platform_program_types(4), Some(15..=22));
    assert_eq!(platform_program_types(18), Some(25..=25));
    assert_eq!(platform_program_types(14), None);
}

/// A minimal record of the given type with one byte of code.
fn typed_record(program_type: u32, code: u8) -> Vec<u8> {
    let mut bytes = words(&[RECORD_VERSION, program_type, 0, 0, 0, 0, 0, 1]);
    bytes.extend([code, 0, 0, 0]);
    bytes
}

/// Two chunks: the first holds the table and a parameter record, the second two programs.
fn chunks() -> Vec<Vec<u8>> {
    let parameter = words(&[RECORD_VERSION, 3, 0]);
    let mut first = words(&[3, 40, 12, 0, 0, 36, 1, 36, 36, 1]);
    first.extend(&parameter);
    let mut second = typed_record(25, 0xaa);
    second.extend(typed_record(16, 0xbb));
    vec![first, second]
}

const fn variant(blob_index: u32, gpu_program_type: i8) -> PlayerSubProgram {
    PlayerSubProgram {
        blob_index,
        keyword_indices: Vec::new(),
        requirements: 0,
        gpu_program_type,
    }
}

#[test]
fn reads_the_records_of_a_platform_variants_and_skips_the_rest() {
    let chunks = chunks();
    let variants = [
        variant(1, SPIRV_PROGRAM),
        variant(2, 16),
        variant(1, SPIRV_PROGRAM),
    ];
    let labelled = variants.iter().enumerate();
    let programs = platform_programs(labelled, &(25..=25), &chunks).unwrap();
    let labels: Vec<usize> = programs.iter().map(|p| p.label).collect();
    assert_eq!(labels, [0, 2]);
    assert_eq!(
        programs[0].entry,
        ProgramEntry {
            segment: 1,
            range: 0..36
        }
    );
    assert_eq!(programs[0].record.program_type, 25);
    assert_eq!(programs[0].code(&chunks), Some(&[0xaa][..]));
    let direct3d = platform_programs(variants.iter().enumerate(), &(15..=22), &chunks).unwrap();
    assert_eq!(direct3d.len(), 1);
    assert_eq!(direct3d[0].code(&chunks), Some(&[0xbb][..]));
}

#[test]
fn rejects_variants_whose_entry_is_missing_or_of_another_type() {
    let chunks = chunks();
    let run = |v: PlayerSubProgram| {
        reason(platform_programs([((), &v)], &(15..=25), &chunks).unwrap_err())
    };
    assert_eq!(
        run(variant(3, 25)),
        "a variant names a blob entry that does not exist"
    );
    assert_eq!(
        run(variant(u32::MAX, 25)),
        "a variant names a blob entry that does not exist"
    );
    assert_eq!(
        run(variant(2, 25)),
        "a variant's record has a different program type"
    );
    assert_eq!(
        run(variant(0, 16)),
        "a variant's record has a different program type"
    );
}

#[test]
fn a_platform_without_chunks_has_no_programs() {
    let none = platform_programs([((), &variant(0, 16))], &(25..=25), &[]).unwrap();
    assert!(none.is_empty());
    let missing = platform_programs([((), &variant(0, 25))], &(25..=25), &[]).unwrap_err();
    assert_eq!(
        reason(missing),
        "a variant names a blob entry that does not exist"
    );
}
