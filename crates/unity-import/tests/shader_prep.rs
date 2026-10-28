use worldrunner_unity_import::shader_prep::{
    PrepareError, ResourceBinding, check_with_naga, prepare,
};

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
    assert!(prepared.bindings.is_empty());
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
    assert_eq!(prepared.bindings, [binding(1, 1, Some(2))]);
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
    assert_eq!(prepared.bindings, [binding(1, 1, Some(2))]);
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

/// A fragment shader with resources in set 0, declared out of binding order: combined
/// samplers B (ID 43, binding 2) and A (ID 44, binding 0), a uniform block (ID 45,
/// binding 1) and a separate image (ID 46, binding 3). Without `combined`, only the uniform
/// block and the separate image.
fn resources(combined: bool) -> Vec<u32> {
    let mut words = [
        op(17, &[1]),
        op(14, &[0, 1]),
        op(15, &[4, 1, 0x6e69_616d, 0, 2]),
        op(16, &[1, 7]),
        op(71, &[2, 30, 0]),
        op(71, &[41, 2]),
        op(72, &[41, 0, 35, 0]),
    ]
    .concat();
    let mut place = |id: u32, binding: u32| {
        words.extend([op(71, &[id, 34, 0]), op(71, &[id, 33, binding])].concat());
    };
    if combined {
        place(43, 2);
        place(44, 0);
    }
    place(45, 1);
    place(46, 3);
    words.extend(
        [
            op(19, &[4]),
            op(33, &[5, 4]),
            op(22, &[7, 32]),
            op(23, &[8, 7, 4]),
            op(32, &[14, 3, 8]),
            op(59, &[14, 2, 3]),
            op(23, &[16, 7, 2]),
            op(43, &[7, 17, 0]),
            op(44, &[16, 18, 17, 17]),
            op(44, &[8, 29, 17, 17, 17, 17]),
            op(25, &[11, 7, 1, 0, 0, 0, 1, 0]),
            op(27, &[12, 11]),
            op(32, &[13, 0, 12]),
            op(32, &[40, 0, 11]),
            op(30, &[41, 8]),
            op(32, &[42, 2, 41]),
        ]
        .concat(),
    );
    if combined {
        words.extend([op(59, &[13, 43, 0]), op(59, &[13, 44, 0])].concat());
    }
    words.extend([op(59, &[42, 45, 2]), op(59, &[40, 46, 0])].concat());
    words.extend([op(54, &[4, 1, 0, 5]), op(248, &[20])].concat());
    let colour = if combined {
        let sample_a = [op(61, &[12, 50, 44]), op(87, &[8, 51, 50, 18])].concat();
        let sample_b = [op(61, &[12, 52, 43]), op(87, &[8, 53, 52, 18])].concat();
        words.extend([sample_a, sample_b].concat());
        51
    } else {
        29
    };
    words.extend([op(62, &[2, colour]), op(253, &[]), op(56, &[])].concat());
    [vec![0x0723_0203, 0x0001_0000, 0, 54, 0], words].concat()
}

fn binding(unity_binding: u32, binding: u32, sampler_binding: Option<u32>) -> ResourceBinding {
    ResourceBinding {
        set: 0,
        unity_binding,
        binding,
        sampler_binding,
    }
}

#[test]
fn maps_every_binding_when_samplers_split_and_later_bindings_move() {
    let raw = resources(true);
    assert_eq!(naga_cause(&raw).as_deref(), Some("parse InvalidId"));
    let prepared = prepare(&raw).unwrap();
    assert_eq!(
        prepared.bindings,
        [
            binding(0, 0, Some(1)),
            binding(1, 2, None),
            binding(2, 3, Some(4)),
            binding(3, 5, None),
        ]
    );
    assert_eq!(naga_cause(&prepared.words), None);
    // spirv-webgpu-transform's hash maps are seeded per map, so repeated runs differ unless
    // preparation puts its output in order.
    for _ in 0..16 {
        assert_eq!(
            prepare(&raw).unwrap(),
            prepared,
            "preparation is deterministic"
        );
    }
}

#[test]
fn separate_images_and_buffers_keep_their_bindings() {
    let raw = resources(false);
    assert_eq!(naga_cause(&raw), None);
    let prepared = prepare(&raw).unwrap();
    assert_eq!(
        prepared.bindings,
        [binding(1, 1, None), binding(3, 3, None)]
    );
    assert_eq!(naga_cause(&prepared.words), None);
}

#[test]
fn leaves_signed_negation_unchanged() {
    let mut words = module(Problems::default());
    // Declare a signed int (ID 31) and negate the constant one (ID 19) into it, before the
    // final store.
    let at = words.len() - 5;
    words.splice(at..at, op(126, &[31, 32, 19]));
    let types = words
        .iter()
        .position(|&w| w == op(22, &[7, 32])[0])
        .unwrap();
    words.splice(types..types, op(21, &[31, 32, 1]));
    words[3] = 33;
    assert_eq!(naga_cause(&words), None);
    assert_eq!(prepare(&words).unwrap().words, words);
}

#[test]
fn a_panic_in_the_sampler_split_is_an_error() {
    // A resource with a descriptor set but no binding, next to an image type.
    let mut words = resources(false);
    let binding_of_45 = op(71, &[45, 33, 1]);
    let at = words.windows(4).position(|w| w == binding_of_45).unwrap();
    words.drain(at..at + 4);
    match prepare(&words) {
        Err(error @ PrepareError::Panicked { .. }) => {
            assert_eq!(error.cause(), "sampler split panic");
        }
        other => panic!("expected a caught panic, got {other:?}"),
    }
}

/// One GLSL.std.450 bit search to test.
#[derive(Clone, Copy)]
struct Search {
    /// 73 `FindILsb`, 74 `FindSMsb` or 75 `FindUMsb`.
    op: u32,
    operand_signed: bool,
    result_signed: bool,
    vector: bool,
}

impl Search {
    /// The operand's ID and type.
    fn operand(self) -> (u32, u32) {
        match (self.vector, self.operand_signed) {
            (false, false) => (19, 6),
            (false, true) => (39, 31),
            (true, false) => (43, 37),
            (true, true) => (44, 38),
        }
    }

    /// The result type and a function pointer to it.
    fn result(self) -> (u32, u32) {
        match (self.vector, self.result_signed) {
            (false, false) => (6, 34),
            (false, true) => (31, 33),
            (true, false) => (37, 46),
            (true, true) => (38, 45),
        }
    }
}

/// A fragment shader storing a bit search of a constant 8 (or a vector of two) into a
/// variable of the search's result type. Integer types: 6 uint, 31 int, 37 uvec2, 38 ivec2.
/// `set_name` is the extended instruction set's name.
fn bit_search(search: Search, set_name: [u32; 4]) -> Vec<u32> {
    let ((operand, _), (result_type, pointer)) = (search.operand(), search.result());
    let words = [
        op(17, &[1]),
        op(11, &[[40].as_slice(), &set_name].concat()),
        op(14, &[0, 1]),
        op(15, &[4, 1, 0x6e69_616d, 0, 2]),
        op(16, &[1, 7]),
        op(71, &[2, 30, 0]),
        op(19, &[4]),
        op(33, &[5, 4]),
        op(21, &[6, 32, 0]),
        op(21, &[31, 32, 1]),
        op(23, &[37, 6, 2]),
        op(23, &[38, 31, 2]),
        op(22, &[7, 32]),
        op(23, &[8, 7, 4]),
        op(32, &[14, 3, 8]),
        op(32, &[33, 7, 31]),
        op(32, &[34, 7, 6]),
        op(32, &[45, 7, 38]),
        op(32, &[46, 7, 37]),
        op(59, &[14, 2, 3]),
        op(43, &[7, 17, 0]),
        op(43, &[6, 19, 8]),
        op(43, &[31, 39, 8]),
        op(44, &[37, 43, 19, 19]),
        op(44, &[38, 44, 39, 39]),
        op(44, &[8, 29, 17, 17, 17, 17]),
        op(54, &[4, 1, 0, 5]),
        op(248, &[20]),
        op(59, &[pointer, 35, 7]),
        op(12, &[result_type, 36, 40, search.op, operand]),
        op(62, &[35, 36]),
        op(62, &[2, 29]),
        op(253, &[]),
        op(56, &[]),
    ]
    .concat();
    [vec![0x0723_0203, 0x0001_0000, 0, 47, 0], words].concat()
}

const GLSL: [u32; 4] = [0x4c53_4c47, 0x6474_732e, 0x3035_342e, 0];

const fn search(op: u32, operand_signed: bool, result_signed: bool, vector: bool) -> Search {
    Search {
        op,
        operand_signed,
        result_signed,
        vector,
    }
}

#[test]
fn casts_bit_searches_whose_result_signedness_differs() {
    for case in [
        search(73, false, true, false),
        search(73, true, false, true),
        search(74, true, false, false),
        search(75, false, true, true),
    ] {
        let raw = bit_search(case, GLSL);
        assert_eq!(
            naga_cause(&raw).as_deref(),
            Some("validate InvalidStoreTypes")
        );
        let prepared = prepare(&raw).unwrap();
        assert_eq!(naga_cause(&prepared.words), None, "search {}", case.op);
        // The search runs in the operand's type into a new ID, then is bitcast.
        let ((operand, operand_type), (result_type, _)) = (case.operand(), case.result());
        assert_eq!(prepared.words[3], 48);
        assert!(contains(
            &prepared.words,
            &op(12, &[operand_type, 47, 40, case.op, operand])
        ));
        assert!(contains(&prepared.words, &op(124, &[result_type, 36, 47])));
    }
}

#[test]
fn leaves_matching_bit_searches_and_other_instruction_sets_alone() {
    for case in [
        search(73, false, false, false),
        search(74, true, true, true),
        search(75, false, false, false),
    ] {
        let raw = bit_search(case, GLSL);
        assert_eq!(naga_cause(&raw), None);
        assert_eq!(prepare(&raw).unwrap().words, raw);
    }
    // An instruction set with another name is not GLSL.std.450, whatever its numbers mean.
    let other = bit_search(
        search(73, false, true, false),
        [0x4c53_4c47, 0x6474_732e, 0x3135_342e, 0],
    );
    assert_eq!(prepare(&other).unwrap().words, other);
}

#[test]
fn rejects_msb_searches_whose_operand_reads_the_other_way() {
    for case in [
        search(74, false, false, false),
        search(75, true, true, true),
    ] {
        let error = prepare(&bit_search(case, GLSL)).unwrap_err();
        assert!(matches!(error, PrepareError::MsbSignedness), "{error}");
        assert_eq!(error.cause(), "Msb signedness");
    }
}

/// A fragment shader that fetches from a sampled 2D image (ID 11) at an integer coordinate,
/// or with `write` writes to a storage 2D image (ID 12), with the image arrayed or not. The
/// coordinate is an `ivec3` (ID 38) for an arrayed image and an `ivec2` otherwise; the module
/// declares no other int vector, so an arrayed access leaves naga without the `ivec2` it
/// splits the coordinate into.
fn image_access(write: bool, arrayed: bool) -> Vec<u32> {
    let size = if arrayed { 3 } else { 2 };
    let layers = u32::from(arrayed);
    let words = [
        op(17, &[1]),
        op(14, &[0, 1]),
        op(15, &[4, 1, 0x6e69_616d, 0, 2]),
        op(16, &[1, 7]),
        op(71, &[2, 30, 0]),
        op(71, &[43, 34, 0]),
        op(71, &[43, 33, 0]),
        op(19, &[4]),
        op(33, &[5, 4]),
        op(22, &[7, 32]),
        op(23, &[8, 7, 4]),
        op(21, &[31, 32, 1]),
        op(23, &[38, 31, size]),
        op(32, &[14, 3, 8]),
        op(59, &[14, 2, 3]),
        op(25, &[11, 7, 1, 0, layers, 0, 1, 0]),
        op(25, &[12, 7, 1, 0, layers, 0, 2, 1]),
        op(32, &[13, 0, if write { 12 } else { 11 }]),
        op(59, &[13, 43, 0]),
        op(43, &[7, 17, 0]),
        op(43, &[31, 39, 0]),
        op(44, &[8, 29, 17, 17, 17, 17]),
        op(44, &[38, 44, 39, 39, 39][..2 + size as usize]),
        op(54, &[4, 1, 0, 5]),
        op(248, &[20]),
    ]
    .concat();
    let access = if write {
        [
            op(61, &[12, 50, 43]),
            op(99, &[50, 44, 29]),
            op(62, &[2, 29]),
        ]
        .concat()
    } else {
        [
            op(61, &[11, 50, 43]),
            op(95, &[8, 51, 50, 44, 2, 39]),
            op(62, &[2, 51]),
        ]
        .concat()
    };
    let end = [op(253, &[]), op(56, &[])].concat();
    [vec![0x0723_0203, 0x0001_0000, 0, 52, 0], words, access, end].concat()
}

/// The vector types a module declares, as their component and size.
fn vector_types(words: &[u32]) -> Vec<[u32; 2]> {
    let mut vectors = Vec::new();
    let mut rest = &words[5..];
    while let Some(&first) = rest.first() {
        let (instruction, next) = rest.split_at((first >> 16) as usize);
        if let &[word, _, component, size] = instruction
            && word & 0xffff == 23
        {
            vectors.push([component, size]);
        }
        rest = next;
    }
    vectors
}

#[test]
fn declares_the_integer_coordinate_vectors_naga_needs() {
    for write in [false, true] {
        let raw = image_access(write, true);
        assert_eq!(
            naga_cause(&raw).as_deref(),
            Some("naga panic"),
            "write {write}"
        );
        let prepared = prepare(&raw).unwrap();
        assert_eq!(naga_cause(&prepared.words), None, "write {write}");
        // An `ivec2` with a new ID, right after the int type it is made of.
        assert_eq!(vector_types(&prepared.words), [[7, 4], [31, 2], [31, 3]]);
        let int = op(21, &[31, 32, 1]);
        let at = prepared.words.windows(4).position(|w| w == int).unwrap() + 4;
        let ivec2 = &prepared.words[at..at + 4];
        assert_eq!(
            [ivec2[0], ivec2[2], ivec2[3]],
            [op(23, &[0, 0, 0])[0], 31, 2]
        );
        assert!((52..prepared.words[3]).contains(&ivec2[1]));
    }
}

#[test]
fn leaves_image_access_alone_when_naga_has_its_coordinate_types() {
    for write in [false, true] {
        let raw = image_access(write, false);
        assert_eq!(naga_cause(&raw), None, "write {write}");
        let prepared = prepare(&raw).unwrap();
        assert_eq!(vector_types(&prepared.words), vector_types(&raw));
    }
}
