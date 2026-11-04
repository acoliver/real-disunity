use worldrunner_unity_import::player_module_resources::{
    ModuleResource, ResourceClass, module_resources,
};

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

/// An image (20), a sampler (21), a block of two members (22) and an array of blocks (23),
/// declared in any order, and an unbound block (24).
fn declarations() -> Vec<Vec<u32>> {
    vec![
        inst(DECORATE, &[20, SET, 0]),
        inst(DECORATE, &[20, BINDING, 2]),
        inst(DECORATE, &[21, SET, 0]),
        inst(DECORATE, &[21, BINDING, 3]),
        inst(DECORATE, &[22, SET, 1]),
        inst(DECORATE, &[22, BINDING, 0]),
        inst(DECORATE, &[23, SET, 1]),
        inst(DECORATE, &[23, BINDING, 1]),
        inst(MEMBER_DECORATE, &[4, 1, OFFSET, 64]),
        inst(MEMBER_DECORATE, &[4, 0, OFFSET, 16]),
        inst(22, &[1, 32]),
        inst(25, &[2, 1, 1, 0, 0, 0, 1, 0]),
        inst(26, &[3]),
        inst(30, &[4, 1, 1]),
        inst(32, &[5, 0, 2]),
        inst(32, &[6, 0, 3]),
        inst(32, &[7, 2, 4]),
        inst(21, &[8, 32, 0]),
        inst(43, &[8, 9, 4]),
        inst(28, &[10, 4, 9]),
        inst(32, &[11, 2, 10]),
        inst(59, &[5, 20, 0]),
        inst(59, &[6, 21, 0]),
        inst(59, &[7, 22, 2]),
        inst(59, &[7, 24, 2]),
        inst(59, &[11, 23, 2]),
    ]
}

#[test]
fn lists_bound_resources_by_set_and_binding_with_block_member_offsets() {
    let resource = |set, binding, class, member_offsets: &[u32]| ModuleResource {
        set,
        binding,
        class,
        member_offsets: member_offsets.to_vec(),
    };
    assert_eq!(
        module_resources(&module(&declarations())),
        [
            resource(0, 2, ResourceClass::Image, &[]),
            resource(0, 3, ResourceClass::Sampler, &[]),
            resource(1, 0, ResourceClass::Block, &[16, 64]),
            resource(1, 1, ResourceClass::Block, &[16, 64]),
        ]
    );
}

#[test]
fn a_module_cut_off_mid_instruction_lists_what_came_before() {
    let mut words = module(&declarations());
    words.truncate(words.len() - 2);
    assert_eq!(module_resources(&words).len(), 3);
    let mut words = module(&declarations());
    words.push(0);
    assert_eq!(module_resources(&words).len(), 4);
    assert!(module_resources(&[]).is_empty());
}
