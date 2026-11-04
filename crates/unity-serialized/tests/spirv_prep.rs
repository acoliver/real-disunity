use worldrunner_unity_serialized::spirv_prep::{PrepareError, prepare};

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
