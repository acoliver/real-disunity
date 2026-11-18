use super::*;
use naga::valid::{Capabilities, ModuleInfo, ValidationFlags, Validator};
use rspirv::dr::{Instruction, Operand};
use rspirv::spirv::{Decoration, Op};

#[path = "../../tests/support/sentinel.rs"]
mod sentinel;

const NAN: u32 = 0x7fc0_0123;
const INFINITY: u32 = 0x7f80_0000;

/// A vertex stage that discards through a helper, as a compiler's output would.
const VERTEX: &str = "
fn discarded() -> vec4<f32> { return vec4<f32>(7.0); }
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    if (i == 1u) { return discarded(); }
    return vec4<f32>(0.25, 0.5, 0.0, 1.0);
}";

/// Two functions that use one constant, and a local variable initialised with a composite.
const SHARED: &str = "
@group(0) @binding(0) var<storage, read_write> out: array<vec4<f32>, 3>;
fn helper() -> vec4<f32> { return vec4<f32>(7.0, 0.5, 7.0, 0.5); }
@compute @workgroup_size(1) fn main() {
    var local = vec4<f32>(7.0, 0.25, 0.25, 7.0);
    out[0] = local;
    out[1] = helper();
    out[2].x = 7.0;
}";

fn vertex() -> Vec<u32> {
    sentinel::spirv(VERTEX, &[(7.0, NAN)])
}

fn shared() -> Vec<u32> {
    sentinel::spirv(SHARED, &[(7.0, INFINITY)])
}

fn parse(words: &[u32]) -> Result<naga::Module, String> {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    naga::front::spv::parse_u8_slice(&bytes, &naga::front::spv::Options::default())
        .map_err(|error| error.to_string())
}

fn validate(module: &naga::Module) -> Result<ModuleInfo, String> {
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(module)
        .map_err(|error| format!("{:?}", error.into_inner()))
}

fn admitted(words: &[u32]) -> (naga::Module, ModuleInfo) {
    let lowered = lower(words).unwrap();
    let module = parse(&lowered).unwrap();
    let info = validate(&module).unwrap_or_else(|error| panic!("refused after lowering: {error}"));
    (module, info)
}

fn assert_unchanged(words: &[u32]) {
    match lower(words).unwrap() {
        Cow::Borrowed(same) => assert_eq!(same, words),
        Cow::Owned(_) => panic!("the words were rewritten"),
    }
}

#[test]
fn naga_refuses_non_finite_constants_in_the_unlowered_module() {
    for words in [vertex(), shared()] {
        let module = parse(&words).unwrap();
        let error = validate(&module).unwrap_err();
        assert!(
            error.contains("NaN") || error.contains("Infinity"),
            "{error}"
        );
    }
}

#[test]
fn lowered_modules_pass_naga_with_their_entry_points() {
    let (module, _) = admitted(&vertex());
    assert_eq!(module.entry_points[0].name, "vs");
    let (module, _) = admitted(&shared());
    assert_eq!(module.entry_points[0].name, "main");
}

#[test]
fn finite_constants_stay_constants_and_no_non_finite_literal_remains() {
    let (module, _) = admitted(&shared());
    let literals: Vec<f32> = module
        .global_expressions
        .iter()
        .filter_map(|(_, expression)| match expression {
            naga::Expression::Literal(naga::Literal::F32(value)) => Some(*value),
            _ => None,
        })
        .collect();
    assert!(
        literals.contains(&0.25) && literals.contains(&0.5),
        "{literals:?}"
    );
    assert!(
        literals.iter().all(|value| value.is_finite()),
        "{literals:?}"
    );
}

#[test]
fn the_lowered_modules_translate_to_msl() {
    for words in [vertex(), shared()] {
        let (module, info) = admitted(&words);
        let (text, _) = naga::back::msl::write_string(
            &module,
            &info,
            &naga::back::msl::Options::default(),
            &naga::back::msl::PipelineOptions::default(),
        )
        .unwrap();
        assert!(text.contains("metal_stdlib"), "{text}");
    }
}

#[test]
fn modules_without_non_finite_constants_are_returned_word_for_word() {
    let finite = sentinel::spirv(SHARED, &[]);
    assert_unchanged(&finite);
    // The bit pattern of infinity as a `u32` is an integer, not a float.
    let integer = sentinel::spirv(
        "@group(0) @binding(0) var<storage, read_write> out: array<u32, 1>;
         @compute @workgroup_size(1) fn main() { out[0] = 0x7f800000u; }",
        &[],
    );
    assert_unchanged(&integer);
    assert_unchanged(&[]);
    assert_unchanged(&[0x0723_0203, 0, 0, 1, 0]);
}

#[test]
fn a_constant_a_global_initializer_needs_is_left_for_naga_to_refuse() {
    let words = sentinel::spirv(
        "var<private> start: vec4<f32> = vec4<f32>(7.0);
         @vertex fn vs() -> @builtin(position) vec4<f32> { return start; }",
        &[(7.0, NAN)],
    );
    assert_unchanged(&words);
    let error = validate(&parse(&words).unwrap()).unwrap_err();
    assert!(error.contains("NaN"), "{error}");
}

/// The module with its non-finite constant declared as a specialization constant, whose default
/// is the same bit pattern.
fn specialized(words: &[u32]) -> Vec<u32> {
    let mut module = rspirv::dr::load_words(words).unwrap();
    let mut ids = Vec::new();
    for i in &mut module.types_global_values {
        if i.class.opcode == Op::Constant && i.operands == [Operand::LiteralBit32(NAN)] {
            *i = Instruction::new(
                Op::SpecConstant,
                i.result_type,
                i.result_id,
                i.operands.clone(),
            );
            ids.extend(i.result_id);
        }
    }
    assert!(!ids.is_empty(), "no constant to specialize");
    for id in ids {
        module.annotations.push(Instruction::new(
            Op::Decorate,
            None,
            None,
            vec![
                Operand::IdRef(id),
                Operand::Decoration(Decoration::SpecId),
                Operand::LiteralBit32(0),
            ],
        ));
    }
    module.assemble()
}

#[test]
fn a_specialization_constant_is_not_rewritten_and_stays_refused() {
    let words = specialized(&vertex());
    assert_unchanged(&words);
    let refused = parse(&words).and_then(|module| validate(&module).map(|_| ()));
    assert!(refused.is_err());
}
