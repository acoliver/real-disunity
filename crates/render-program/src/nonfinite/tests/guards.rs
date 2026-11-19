//! Nested composites shared by functions and used by an `OpPhi` are rebuilt in each function,
//! and specialization constants stay out of the rewrite.

use std::collections::BTreeSet;

use rspirv::dr::{Builder, Function, Instruction, Module, Operand};
use rspirv::spirv::{
    AddressingModel, Capability, Decoration, ExecutionMode, ExecutionModel, FunctionControl,
    MemoryModel, Op, SelectionControl, StorageClass,
};

use super::*;
use crate::spirv::id;

/// A fragment stage with two functions that use one constant: an array of two vectors, the first
/// of them holding a NaN, which `main` also passes to an `OpPhi` at a selection merge.
struct Nested {
    words: Vec<u32>,
    outer: u32,
    inner: u32,
}

struct Types {
    void: u32,
    float: u32,
    vector: u32,
    array: u32,
    boolean: u32,
}

/// The constants `main` and `helper` read: `outer` holds `inner`, and `other` is all finite.
struct Constants {
    half: u32,
    inner: u32,
    outer: u32,
    other: u32,
}

/// The interface variables: a float input and two vector outputs.
struct Interface {
    input: u32,
    outputs: [u32; 2],
}

fn declare_types(b: &mut Builder) -> Types {
    let void = b.type_void();
    let float = b.type_float(32, None);
    let vector = b.type_vector(float, 2);
    let uint = b.type_int(32, 0);
    let two = b.constant_bit32(uint, 2);
    let array = b.type_array(vector, two);
    let boolean = b.type_bool();
    Types {
        void,
        float,
        vector,
        array,
        boolean,
    }
}

fn declare_constants(b: &mut Builder, t: &Types) -> Constants {
    let [nan, half, quarter, rest] = [NAN, 0.5f32.to_bits(), 0.25f32.to_bits(), 0.75f32.to_bits()]
        .map(|bits| b.constant_bit32(t.float, bits));
    let inner = b.constant_composite(t.vector, [nan, half]);
    let finite = b.constant_composite(t.vector, [quarter, rest]);
    let outer = b.constant_composite(t.array, [inner, finite]);
    let other = b.constant_composite(t.array, [finite, finite]);
    Constants {
        half,
        inner,
        outer,
        other,
    }
}

fn declare_interface(b: &mut Builder, t: &Types) -> Interface {
    let input_pointer = b.type_pointer(None, StorageClass::Input, t.float);
    let output_pointer = b.type_pointer(None, StorageClass::Output, t.vector);
    let input = b.variable(input_pointer, None, StorageClass::Input, None);
    b.decorate(input, Decoration::Location, [Operand::LiteralBit32(0)]);
    let outputs = [0, 1].map(|location| {
        let output = b.variable(output_pointer, None, StorageClass::Output, None);
        b.decorate(
            output,
            Decoration::Location,
            [Operand::LiteralBit32(location)],
        );
        output
    });
    Interface { input, outputs }
}

/// A function that returns the first vector of the nested constant.
fn define_helper(b: &mut Builder, t: &Types, outer: u32) -> u32 {
    let function_type = b.type_function(t.vector, []);
    let helper = b
        .begin_function(t.vector, None, FunctionControl::NONE, function_type)
        .unwrap();
    b.begin_block(None).unwrap();
    let first = b.composite_extract(t.vector, None, outer, [0]).unwrap();
    b.ret_value(first).unwrap();
    b.end_function().unwrap();
    helper
}

/// The entry point: a selection whose merge has an `OpPhi` of two arrays, then a call.
fn define_main(
    b: &mut Builder,
    t: &Types,
    c: &Constants,
    interface: &Interface,
    helper: u32,
) -> u32 {
    let function_type = b.type_function(t.void, []);
    let main = b
        .begin_function(t.void, None, FunctionControl::NONE, function_type)
        .unwrap();
    let (then, merge) = (b.id(), b.id());
    let entry = b.begin_block(None).unwrap();
    let loaded = b.load(t.float, None, interface.input, None, []).unwrap();
    let below = b.f_ord_less_than(t.boolean, None, loaded, c.half).unwrap();
    b.selection_merge(merge, SelectionControl::NONE).unwrap();
    b.branch_conditional(below, then, merge, []).unwrap();
    b.begin_block(Some(then)).unwrap();
    b.branch(merge).unwrap();
    b.begin_block(Some(merge)).unwrap();
    let incoming = [(c.outer, then), (c.other, entry)];
    let chosen = b.phi(t.array, None, incoming).unwrap();
    let from_phi = b.composite_extract(t.vector, None, chosen, [0]).unwrap();
    b.store(interface.outputs[0], from_phi, None, []).unwrap();
    let called = b.function_call(t.vector, None, helper, []).unwrap();
    b.store(interface.outputs[1], called, None, []).unwrap();
    b.ret().unwrap();
    b.end_function().unwrap();
    main
}

fn nested() -> Nested {
    let mut b = Builder::new();
    b.set_version(1, 3);
    b.capability(Capability::Shader);
    b.memory_model(AddressingModel::Logical, MemoryModel::GLSL450);
    let types = declare_types(&mut b);
    let constants = declare_constants(&mut b, &types);
    let interface = declare_interface(&mut b, &types);
    let helper = define_helper(&mut b, &types, constants.outer);
    let main = define_main(&mut b, &types, &constants, &interface, helper);
    let variables = [interface.input, interface.outputs[0], interface.outputs[1]];
    b.entry_point(ExecutionModel::Fragment, main, "main", variables);
    b.execution_mode(main, ExecutionMode::OriginUpperLeft, []);
    Nested {
        words: b.module().assemble(),
        outer: constants.outer,
        inner: constants.inner,
    }
}

fn loaded(words: &[u32]) -> Module {
    rspirv::dr::load_words(words).unwrap()
}

fn mentions(module: &Module, wanted: u32) -> bool {
    module
        .all_inst_iter()
        .any(|i| i.operands.contains(&Operand::IdRef(wanted)))
}

/// The ids of the compositions a function builds, which must all be in its entry block.
fn rebuilt(function: &Function) -> BTreeSet<u32> {
    let built = |i: &&Instruction| i.class.opcode == Op::CompositeConstruct;
    let anywhere: BTreeSet<u32> = function
        .all_inst_iter()
        .filter(built)
        .filter_map(|i| i.result_id)
        .collect();
    let entry: BTreeSet<u32> = function.blocks[0]
        .instructions
        .iter()
        .filter(built)
        .filter_map(|i| i.result_id)
        .collect();
    assert_eq!(anywhere, entry, "a value is built outside the entry block");
    entry
}

/// Every value a function defines is defined before an instruction that reads it, in the order
/// the blocks are written.
fn assert_definitions_precede_uses(function: &Function) {
    let values: BTreeSet<u32> = function
        .all_inst_iter()
        .filter(|i| i.class.opcode != Op::Label)
        .filter_map(|i| i.result_id)
        .collect();
    let mut seen = BTreeSet::new();
    for i in function.all_inst_iter() {
        for used in i
            .operands
            .iter()
            .filter_map(id)
            .filter(|u| values.contains(u))
        {
            assert!(seen.contains(&used), "%{used} is read before it is defined");
        }
        seen.extend(i.result_id);
    }
}

fn phi(module: &Module) -> Instruction {
    let mut phis = module.all_inst_iter().filter(|i| i.class.opcode == Op::Phi);
    phis.next().expect("the module has an OpPhi").clone()
}

fn phi_labels(phi: &Instruction) -> Vec<&Operand> {
    phi.operands.iter().skip(1).step_by(2).collect()
}

#[test]
fn a_nested_composite_used_by_two_functions_and_a_phi_is_rebuilt_in_each_function() {
    let nested = nested();
    admitted(&nested.words);
    let before = loaded(&nested.words);
    let after = loaded(&lower(&nested.words).unwrap());
    assert_eq!(after.functions.len(), 2);
    let sets: Vec<BTreeSet<u32>> = after.functions.iter().map(rebuilt).collect();
    for built in &sets {
        assert!(built.len() >= 2, "the array and its vector: {built:?}");
    }
    assert!(sets[0].is_disjoint(&sets[1]), "{sets:?}");
    after
        .functions
        .iter()
        .for_each(assert_definitions_precede_uses);
    assert!(!mentions(&after, nested.outer) && !mentions(&after, nested.inner));

    let (old, new) = (phi(&before), phi(&after));
    assert_eq!(phi_labels(&old), phi_labels(&new));
    assert_eq!(
        old.operands[2], new.operands[2],
        "the finite incoming value changed"
    );
    let Operand::IdRef(incoming) = new.operands[0] else {
        panic!("{new:?}");
    };
    assert_ne!(old.operands[0], new.operands[0]);
    assert!(sets.iter().any(|built| built.contains(&incoming)));
}

const BOTH: &str = "
@group(0) @binding(0) var<storage, read_write> out: array<f32, 2>;
@compute @workgroup_size(1) fn main() { out[0] = 7.0; out[1] = 9.0; }";

fn spec_constants(module: &Module) -> Vec<Instruction> {
    let spec = |i: &&Instruction| i.class.opcode == Op::SpecConstant;
    module
        .types_global_values
        .iter()
        .filter(spec)
        .cloned()
        .collect()
}

fn spec_ids(module: &Module) -> Vec<Instruction> {
    let is_spec_id =
        |i: &&Instruction| i.operands.get(1) == Some(&Operand::Decoration(Decoration::SpecId));
    module
        .annotations
        .iter()
        .filter(is_spec_id)
        .cloned()
        .collect()
}

#[test]
fn an_ordinary_constant_is_lowered_beside_a_specialization_constant_that_stays_refused() {
    let words = specialized_bits(
        &sentinel::spirv(BOTH, &[(7.0, NAN), (9.0, INFINITY)]),
        INFINITY,
    );
    let before = loaded(&words);
    let lowered = lower(&words).unwrap();
    assert!(matches!(lowered, Cow::Owned(_)));
    let after = loaded(&lowered);
    let nan = before
        .types_global_values
        .iter()
        .find(|i| i.operands == [Operand::LiteralBit32(NAN)])
        .and_then(|i| i.result_id)
        .unwrap();
    assert!(!after.all_inst_iter().any(|i| i.result_id == Some(nan)));
    assert_eq!(spec_constants(&before).len(), 1);
    assert_eq!(spec_constants(&before), spec_constants(&after));
    assert_eq!(spec_ids(&before).len(), 1);
    assert_eq!(spec_ids(&before), spec_ids(&after));
    let error = parse(&lowered)
        .and_then(|module| validate(&module).map(|_| ()))
        .unwrap_err();
    assert!(
        error.contains("Literal") && error.contains("Infinity"),
        "{error}"
    );
}

#[test]
fn a_specialization_composite_with_a_non_finite_part_leaves_the_module_unchanged() {
    let mut module = loaded(&shared());
    let non_finite: Vec<u32> = module
        .types_global_values
        .iter()
        .filter(|i| i.operands == [Operand::LiteralBit32(INFINITY)])
        .filter_map(|i| i.result_id)
        .collect();
    let composite = module
        .types_global_values
        .iter_mut()
        .find(|i| {
            i.class.opcode == Op::ConstantComposite
                && i.operands
                    .iter()
                    .filter_map(id)
                    .any(|part| non_finite.contains(&part))
        })
        .expect("a composite with an infinite part");
    *composite = Instruction::new(
        Op::SpecConstantComposite,
        composite.result_type,
        composite.result_id,
        composite.operands.clone(),
    );
    assert_unchanged(&module.assemble());
}
