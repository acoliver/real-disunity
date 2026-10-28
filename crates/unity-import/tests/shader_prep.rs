use worldrunner_unity_import::shader_prep::{PrepareError, check_with_naga, prepare};

/// Which of the three things naga cannot read a test module contains.
#[derive(Clone, Copy, Default)]
struct Problems {
    combined_sampler: bool,
    spec_array: bool,
    unsigned_negate: bool,
}

fn op(code: u32, operands: &[u32]) -> Vec<u32> {
    let len = u32::try_from(operands.len() + 1).unwrap();
    [vec![(len << 16) | code], operands.to_vec()].concat()
}

/// The declarations of the test module. IDs: 1 main, 2 output, 3 texture, 4 void,
/// 5 function type, 6 uint, 7 float, 8 vec4, 9 array length, 10 array, 11 image, 12 sampled
/// image, 13 its pointer, 14 output pointer, 15 array pointer, 16 vec2, 17 zero,
/// 18 coordinate, 19 one, 25 uint pointer, 29 constant colour.
fn globals(problems: Problems) -> Vec<u32> {
    let mut words = [
        op(17, &[1]),
        op(14, &[0, 1]),
        op(15, &[4, 1, 0x6e69_616d, 0, 2]),
        op(16, &[1, 7]),
        op(71, &[2, 30, 0]),
    ]
    .concat();
    if problems.spec_array {
        words.extend(op(71, &[9, 1, 3]));
    }
    if problems.combined_sampler {
        words.extend([op(71, &[3, 34, 0]), op(71, &[3, 33, 1])].concat());
    }
    let length = if problems.spec_array { 50 } else { 43 };
    words.extend(
        [
            op(19, &[4]),
            op(33, &[5, 4]),
            op(21, &[6, 32, 0]),
            op(22, &[7, 32]),
            op(23, &[8, 7, 4]),
            op(length, &[6, 9, 2]),
            op(28, &[10, 7, 9]),
            op(32, &[14, 3, 8]),
            op(59, &[14, 2, 3]),
            op(32, &[15, 7, 10]),
            op(32, &[25, 7, 6]),
            op(23, &[16, 7, 2]),
            op(43, &[7, 17, 0]),
            op(44, &[16, 18, 17, 17]),
            op(43, &[6, 19, 1]),
            op(44, &[8, 29, 17, 17, 17, 17]),
        ]
        .concat(),
    );
    if problems.combined_sampler {
        let image = op(25, &[11, 7, 1, 0, 0, 0, 1, 0]);
        words.extend(
            [
                image,
                op(27, &[12, 11]),
                op(32, &[13, 0, 12]),
                op(59, &[13, 3, 0]),
            ]
            .concat(),
        );
    }
    words
}

/// The test module's function. IDs: 20 label, 21 array variable, 22 loaded sampler,
/// 23 colour, 24 negation, 26 uint variable.
fn body(problems: Problems) -> Vec<u32> {
    let mut words = [
        op(54, &[4, 1, 0, 5]),
        op(248, &[20]),
        op(59, &[15, 21, 7]),
        op(59, &[25, 26, 7]),
    ]
    .concat();
    let colour = if problems.combined_sampler {
        words.extend([op(61, &[12, 22, 3]), op(87, &[8, 23, 22, 18])].concat());
        23
    } else {
        29
    };
    if problems.unsigned_negate {
        words.extend([op(126, &[6, 24, 19]), op(62, &[26, 24])].concat());
    }
    words.extend([op(62, &[2, colour]), op(253, &[]), op(56, &[])].concat());
    words
}

/// A fragment shader writing a colour, assembled by hand, with the given problems.
fn module(problems: Problems) -> Vec<u32> {
    [
        vec![0x0723_0203, 0x0001_0000, 0, 30, 0],
        globals(problems),
        body(problems),
    ]
    .concat()
}

/// Whether `words` contains the instruction `instruction`.
fn contains(words: &[u32], instruction: &[u32]) -> bool {
    words.windows(instruction.len()).any(|w| w == instruction)
}

fn naga_cause(words: &[u32]) -> Option<String> {
    check_with_naga(words).err().map(|error| error.cause())
}

#[test]
fn naga_accepts_the_module_without_problems_unchanged_in_meaning() {
    let plain = module(Problems::default());
    assert_eq!(naga_cause(&plain), None);
    let prepared = prepare(&plain).unwrap();
    assert!(prepared.split_samplers.is_empty());
    assert_eq!(prepared.words, plain);
}

#[test]
fn splits_combined_samplers_and_records_their_bindings() {
    let raw = module(Problems {
        combined_sampler: true,
        ..Problems::default()
    });
    assert_eq!(naga_cause(&raw).as_deref(), Some("parse InvalidId"));
    let prepared = prepare(&raw).unwrap();
    assert_eq!(prepared.split_samplers, [(0, 1)]);
    assert_eq!(naga_cause(&prepared.words), None);
}

#[test]
fn gives_specialization_constants_their_defaults() {
    let raw = module(Problems {
        spec_array: true,
        ..Problems::default()
    });
    assert_eq!(naga_cause(&raw).as_deref(), Some("parse InvalidArraySize"));
    let prepared = prepare(&raw).unwrap();
    assert_eq!(prepared.words, module(Problems::default()));
    assert_eq!(naga_cause(&prepared.words), None);
}

#[test]
fn lowers_unsigned_negation_to_a_subtraction_from_zero() {
    let raw = module(Problems {
        unsigned_negate: true,
        ..Problems::default()
    });
    assert_eq!(
        naga_cause(&raw).as_deref(),
        Some("validate InvalidUnaryOperandType")
    );
    let prepared = prepare(&raw).unwrap();
    assert_eq!(naga_cause(&prepared.words), None);
    // One new ID, a null uint constant before the function, and `0 - x`.
    assert_eq!(prepared.words[3], 31);
    let words = &prepared.words;
    assert!(contains(words, &op(46, &[6, 30])));
    assert!(contains(words, &op(130, &[6, 24, 30, 19])));
    assert!(!contains(words, &op(126, &[6, 24, 19])));
}

#[test]
fn handles_every_problem_together() {
    let all = Problems {
        combined_sampler: true,
        spec_array: true,
        unsigned_negate: true,
    };
    let prepared = prepare(&module(all)).unwrap();
    assert_eq!(prepared.split_samplers, [(0, 1)]);
    assert_eq!(naga_cause(&prepared.words), None);
}

#[test]
fn rejects_modules_that_are_not_structurally_valid() {
    let mut broken = module(Problems::default());
    // The final `OpFunctionEnd` claims a second word that is not there.
    let last = broken.last_mut().unwrap();
    assert_eq!([*last], *op(56, &[]));
    *last = op(56, &[0])[0];
    assert!(matches!(prepare(&broken), Err(PrepareError::Module(_))));
    assert!(matches!(
        prepare(&broken[..4]),
        Err(PrepareError::Module(_))
    ));
}
