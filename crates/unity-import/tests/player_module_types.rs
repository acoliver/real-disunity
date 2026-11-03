use worldrunner_unity_import::player_module_types::{BlockTree, Field, Shape, block_trees};

/// A SPIR-V instruction: the word count and opcode, then the operands.
fn inst(opcode: u32, operands: &[u32]) -> Vec<u32> {
    let count = u32::try_from(operands.len() + 1).unwrap();
    let mut words = vec![count << 16 | opcode];
    words.extend(operands);
    words
}

fn module(instructions: &[Vec<u32>]) -> Vec<u32> {
    let mut words = vec![0x0723_0203, 0x0001_0300, 0, 100, 0];
    words.extend(instructions.concat());
    words
}

const DECORATE: u32 = 71;
const MEMBER_DECORATE: u32 = 72;
const SET: u32 = 34;
const BINDING: u32 = 33;
const OFFSET: u32 = 35;
const ARRAY_STRIDE: u32 = 6;
const MATRIX_STRIDE: u32 = 7;
const UNIFORM: u32 = 2;

const FLOAT: u32 = 1;
const VEC4: u32 = 2;
const MAT4: u32 = 3;
const INT: u32 = 4;
const THREE: u32 = 5;
const INNER: u32 = 6;
const INNER_ARRAY: u32 = 7;
const BLOCK: u32 = 8;
const POINTER: u32 = 9;
const BLOCK_VARIABLE: u32 = 10;
const NESTED: u32 = 11;
const NESTED_POINTER: u32 = 12;

/// A block at set 1 binding 4 with a vector, an inner struct of a float and a vector (size 32),
/// an array of three inner structs at stride 48 and a matrix with a stride of 16.
fn declarations() -> Vec<Vec<u32>> {
    vec![
        inst(DECORATE, &[BLOCK_VARIABLE, SET, 1]),
        inst(DECORATE, &[BLOCK_VARIABLE, BINDING, 4]),
        inst(DECORATE, &[INNER_ARRAY, ARRAY_STRIDE, 48]),
        inst(MEMBER_DECORATE, &[INNER, 0, OFFSET, 0]),
        inst(MEMBER_DECORATE, &[INNER, 1, OFFSET, 16]),
        inst(MEMBER_DECORATE, &[BLOCK, 0, OFFSET, 0]),
        inst(MEMBER_DECORATE, &[BLOCK, 1, OFFSET, 16]),
        inst(MEMBER_DECORATE, &[BLOCK, 2, OFFSET, 64]),
        inst(MEMBER_DECORATE, &[BLOCK, 3, OFFSET, 208]),
        inst(MEMBER_DECORATE, &[BLOCK, 3, MATRIX_STRIDE, 16]),
        inst(22, &[FLOAT, 32]),
        inst(23, &[VEC4, FLOAT, 4]),
        inst(24, &[MAT4, VEC4, 4]),
        inst(21, &[INT, 32, 0]),
        inst(43, &[INT, THREE, 3]),
        inst(30, &[INNER, FLOAT, VEC4]),
        inst(28, &[INNER_ARRAY, INNER, THREE]),
        inst(30, &[BLOCK, VEC4, INNER, INNER_ARRAY, MAT4]),
        inst(32, &[POINTER, UNIFORM, BLOCK]),
        inst(59, &[POINTER, BLOCK_VARIABLE, UNIFORM]),
    ]
}

fn field(offset: u32, shape: Shape) -> Field {
    Field {
        offset: Some(offset),
        matrix_stride: None,
        shape,
    }
}

fn vec4() -> Shape {
    Shape::Vector {
        count: 4,
        bytes: 4,
    }
}

fn inner() -> Shape {
    Shape::Struct(vec![
        field(0, Shape::Scalar { bytes: 4 }),
        field(16, vec4()),
    ])
}

#[test]
fn a_block_lists_its_members_with_offsets_nested_structs_array_strides_and_matrix_strides() {
    let expected = BlockTree {
        set: 1,
        binding: 4,
        fields: vec![
            field(0, vec4()),
            field(16, inner()),
            field(
                64,
                Shape::Array {
                    element: Box::new(inner()),
                    length: Some(3),
                    stride: Some(48),
                },
            ),
            Field {
                offset: Some(208),
                matrix_stride: Some(16),
                shape: Shape::Matrix {
                    columns: 4,
                    rows: 4,
                },
            },
        ],
    };
    assert_eq!(block_trees(&module(&declarations())), [expected]);
}

#[test]
fn a_struct_spans_to_the_end_of_its_last_member() {
    let trees = block_trees(&module(&declarations()));
    let fields = &trees[0].fields;
    assert_eq!(fields[1].span(), 32);
    assert_eq!(fields[2].span(), 144);
    assert_eq!(fields[3].span(), 64);
    assert_eq!(fields[0].span(), 16);
}

#[test]
fn an_array_of_blocks_and_an_unbound_block_are_read_as_the_block_or_left_out() {
    let mut instructions = declarations();
    // An array of the block type: still the block. A second block variable with no binding.
    instructions.push(inst(28, &[NESTED, BLOCK, THREE]));
    instructions.push(inst(32, &[NESTED_POINTER, UNIFORM, NESTED]));
    instructions.push(inst(59, &[NESTED_POINTER, 13, UNIFORM]));
    instructions.push(inst(59, &[POINTER, 14, UNIFORM]));
    instructions.push(inst(DECORATE, &[13, SET, 0]));
    instructions.push(inst(DECORATE, &[13, BINDING, 0]));
    let trees = block_trees(&module(&instructions));
    let bindings: Vec<(u32, u32)> = trees.iter().map(|t| (t.set, t.binding)).collect();
    assert_eq!(bindings, [(0, 0), (1, 4)]);
}

#[test]
fn a_member_without_an_offset_has_none() {
    let mut instructions = declarations();
    instructions.retain(|i| i != &inst(MEMBER_DECORATE, &[BLOCK, 2, OFFSET, 64]));
    let trees = block_trees(&module(&instructions));
    assert_eq!(trees[0].fields[2].offset, None);
    assert_eq!(trees[0].fields[1].offset, Some(16));
}

#[test]
fn a_module_cut_off_mid_instruction_lists_what_came_before() {
    let mut words = module(&declarations());
    words.truncate(words.len() - 2);
    assert!(block_trees(&words).is_empty());
}
