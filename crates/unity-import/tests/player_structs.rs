use worldrunner_unity_import::player_structs::{StructReport, fact};
use worldrunner_unity_import::player_variants::RawStage;
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, StructLayout,
    VariantParameters,
};

/// A SPIR-V instruction: the word count and opcode, then the operands.
fn inst(opcode: u32, operands: &[u32]) -> Vec<u32> {
    let count = u32::try_from(operands.len() + 1).unwrap();
    let mut words = vec![count << 16 | opcode];
    words.extend(operands);
    words
}

const DECORATE: u32 = 71;
const MEMBER_DECORATE: u32 = 72;
const BINDING: u32 = 33;
const SET: u32 = 34;
const OFFSET: u32 = 35;
const ARRAY_STRIDE: u32 = 6;
const UNIFORM: u32 = 2;

/// IDs: 1 float, 2 float4, 3 int4 element type, 4 int, 5 the struct, 6 the constant 2, 7 the
/// array of the struct, 8 the block, 9 its pointer, 10 the variable, 11 signed int.
///
/// The block holds one array of two structs at offset 0. A struct is a float4 at 0 and an
/// int4 at 16, and the array's stride is `stride`.
fn block_module(stride: u32, second: u32) -> Vec<u32> {
    let mut words = vec![0x0723_0203, 0x0001_0300, 0, 100, 0];
    for instruction in [
        inst(DECORATE, &[10, SET, 0]),
        inst(DECORATE, &[10, BINDING, 0]),
        inst(DECORATE, &[7, ARRAY_STRIDE, stride]),
        inst(MEMBER_DECORATE, &[5, 0, OFFSET, 0]),
        inst(MEMBER_DECORATE, &[5, 1, OFFSET, second]),
        inst(MEMBER_DECORATE, &[8, 0, OFFSET, 0]),
        inst(22, &[1, 32]),
        inst(23, &[2, 1, 4]),
        inst(21, &[4, 32, 1]),
        inst(23, &[3, 4, 4]),
        inst(30, &[5, 2, 3]),
        inst(21, &[11, 32, 0]),
        inst(43, &[11, 6, 2]),
        inst(28, &[7, 5, 6]),
        inst(30, &[8, 7]),
        inst(32, &[9, UNIFORM, 8]),
        inst(59, &[9, 10, UNIFORM]),
    ] {
        words.extend(instruction);
    }
    words
}

fn stage(words: Vec<u32>) -> RawStage {
    RawStage { slot: 0, words }
}

fn member(name: &str, kind: i32, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind,
        rows: 1,
        columns: 4,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

fn parameters(item: StructLayout, buffer_size: i32) -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![ConstantBuffer {
            name: "Items".to_owned(),
            size: buffer_size,
            members: Vec::new(),
            structs: vec![item],
        }],
        bindings: vec![ResourceBinding {
            name: "Items".to_owned(),
            slot: DescriptorSlot {
                binding: 0,
                set: 0,
                stages: DescriptorSlot::VERTEX,
            },
            kind: ResourceKind::ConstantBuffer,
        }],
    }
}

fn item() -> StructLayout {
    StructLayout {
        name: "Item".to_owned(),
        index: 0,
        array_size: 2,
        size: 32,
        members: vec![member("_Pos", 0, 0), member("_Mask", 1, 16)],
    }
}

fn report(parameters: &VariantParameters, words: Vec<u32>) -> StructReport {
    let mut report = StructReport::default();
    report.add_variant(parameters, &[stage(words)]);
    report
}

fn tally(report: &StructReport, name: &str) -> (usize, usize) {
    let found = report.facts[name];
    (found.matched, found.total)
}

#[test]
fn a_buffer_the_rule_places_as_the_block_has_it_is_explained_with_every_member() {
    let report = report(&parameters(item(), 64), block_module(32, 16));
    assert_eq!((report.buffers.matched, report.buffers.total), (1, 1));
    assert_eq!((report.members.matched, report.members.total), (2, 2));
    assert!(report.unexplained.is_empty() && report.unexplained_members.is_empty());
    for name in fact::ALL {
        assert_eq!(tally(&report, name), (1, 1), "{name}");
    }
}

#[test]
fn members_in_another_order_than_the_blocks_are_counted_apart_and_still_explained() {
    let mut reversed = item();
    reversed.members.reverse();
    let report = report(&parameters(reversed, 64), block_module(32, 16));
    assert_eq!((report.buffers.matched, report.buffers.total), (1, 1));
    assert_eq!(tally(&report, fact::MEMBERS_IN_BLOCK_ORDER), (0, 1));
    assert_eq!(tally(&report, fact::MEMBER_OFFSETS_ARE_THE_TYPES), (1, 1));
}

#[test]
fn a_stride_that_differs_from_the_structs_size_leaves_the_buffer_unexplained_by_cause() {
    let report = report(&parameters(item(), 64), block_module(48, 16));
    assert_eq!((report.buffers.matched, report.buffers.total), (0, 1));
    assert_eq!(tally(&report, fact::SIZE_IS_ARRAY_STRIDE), (0, 1));
    assert_eq!(
        report.unexplained.iter().collect::<Vec<_>>(),
        [(&fact::SIZE_IS_ARRAY_STRIDE.to_owned(), &1)]
    );
}

#[test]
fn a_member_the_block_does_not_have_at_its_offset_is_unexplained_by_cause() {
    let mut moved = item();
    moved.members[1].offset = 20;
    let report = report(&parameters(moved, 64), block_module(32, 16));
    assert_eq!((report.members.matched, report.members.total), (1, 2));
    assert_eq!(
        report.unexplained_members.iter().collect::<Vec<_>>(),
        [(&"no member of the struct type at the offset".to_owned(), &1)]
    );
    assert_eq!((report.buffers.matched, report.buffers.total), (0, 1));
}

#[test]
fn a_member_of_another_type_than_the_blocks_is_unexplained_by_cause() {
    let mut wrong = item();
    wrong.members[1].kind = 0;
    let report = report(&parameters(wrong, 64), block_module(32, 16));
    assert_eq!((report.members.matched, report.members.total), (1, 2));
    assert_eq!(
        report.unexplained_members.iter().collect::<Vec<_>>(),
        [(
            &"type of the member differs from the struct type's".to_owned(),
            &1
        )]
    );
}

#[test]
fn a_struct_the_packer_refuses_is_unexplained_with_the_refusal_as_its_cause() {
    let mut moved = item();
    moved.index = 16;
    let report = report(&parameters(moved, 64), block_module(32, 16));
    assert_eq!((report.buffers.matched, report.buffers.total), (0, 1));
    assert_eq!(
        report.unexplained.iter().collect::<Vec<_>>(),
        [(&"the packer refuses the struct".to_owned(), &1)]
    );
}

#[test]
fn a_buffer_without_a_block_in_the_stages_is_unexplained_by_cause() {
    let mut other = parameters(item(), 64);
    other.bindings[0].slot.binding = 3;
    let report = report(&other, block_module(32, 16));
    assert_eq!((report.buffers.matched, report.buffers.total), (0, 1));
    assert_eq!(
        report.unexplained.iter().collect::<Vec<_>>(),
        [(
            &"no block bound in a stage the binding names".to_owned(),
            &1
        )]
    );
}

#[test]
fn a_buffer_size_other_than_the_arrays_leaves_the_buffer_unexplained() {
    let report = report(&parameters(item(), 80), block_module(32, 16));
    assert_eq!(tally(&report, fact::BUFFER_SIZE_IS_THE_ARRAYS), (0, 1));
    assert_eq!((report.buffers.matched, report.buffers.total), (0, 1));
}

#[test]
fn buffers_without_structs_are_not_counted() {
    let mut plain = parameters(item(), 64);
    plain.constant_buffers[0].structs.clear();
    let report = report(&plain, block_module(32, 16));
    assert_eq!(report.buffers.total, 0);
    assert!(report.facts.values().all(|t| t.total == 0));
}

#[test]
fn member_shapes_are_counted_against_the_blocks_types() {
    let report = report(&parameters(item(), 64), block_module(32, 16));
    assert_eq!(
        report.shapes.iter().collect::<Vec<_>>(),
        [
            (&"kind 0 1x4 <-> float x4".to_owned(), &1),
            (&"kind 1 1x4 <-> signed x4".to_owned(), &1),
        ]
    );
}

#[test]
fn names_are_counted_against_material_properties_and_other_buffers_members() {
    let mut parameters = parameters(item(), 64);
    parameters.constant_buffers.push(ConstantBuffer {
        name: "Other".to_owned(),
        size: 16,
        members: vec![member("_Pos", 0, 0)],
        structs: Vec::new(),
    });
    let mut report = report(&parameters, block_module(32, 16));
    report.add_material_names(["_Mask".to_owned(), "_Tint".to_owned()]);
    let names = report.names();
    assert_eq!(names.members, 2);
    assert_eq!(names.structs, 1);
    assert_eq!(names.members_that_are_top_level_members, 1);
    assert_eq!(names.members_that_are_material_properties, 1);
    assert_eq!(names.members_that_are_struct_names, 0);
    assert_eq!(names.members_with_a_suffix_after_a_known_name, 0);
}
