use worldrunner_unity_import::{
    player_module_resources::{ModuleResource, ResourceClass},
    player_parameters::{ParameterReport, StageModule, Tally},
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, VariantParameters,
};

const VERTEX: u8 = DescriptorSlot::VERTEX;
const FRAGMENT: u8 = DescriptorSlot::FRAGMENT;

fn slot(set: u16, binding: u16, stages: u8) -> DescriptorSlot {
    DescriptorSlot {
        binding,
        set,
        stages,
    }
}

fn member(name: &str, offset: i32) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind: 0,
        rows: 1,
        columns: 4,
        is_matrix: false,
        array_size: 0,
        offset,
    }
}

fn bind(name: &str, slot: DescriptorSlot, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        name: name.to_owned(),
        slot,
        kind,
    }
}

/// A texture read with a separate sampler in the fragment stage and a constant buffer of two
/// members in both stages.
fn parameters() -> VariantParameters {
    let texture = ResourceKind::Texture {
        sampler: Some(slot(0, 3, FRAGMENT)),
        dimension: 4,
    };
    VariantParameters {
        constant_buffers: vec![ConstantBuffer {
            name: "Block".to_owned(),
            size: 32,
            members: vec![member("a", 0), member("b", 16)],
            structs: Vec::new(),
        }],
        bindings: vec![
            bind("Tex", slot(0, 2, FRAGMENT), texture),
            bind(
                "Block",
                slot(1, 0, VERTEX | FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
        ],
    }
}

fn resource(set: u32, binding: u32, class: ResourceClass, offsets: &[u32]) -> ModuleResource {
    ModuleResource {
        set,
        binding,
        class,
        member_offsets: offsets.to_vec(),
    }
}

fn block(offsets: &[u32]) -> ModuleResource {
    resource(1, 0, ResourceClass::Block, offsets)
}

fn modules(fragment: Vec<ModuleResource>) -> Vec<StageModule> {
    vec![
        StageModule {
            slot: 0,
            resources: vec![block(&[0, 16])],
        },
        StageModule {
            slot: 1,
            resources: fragment,
        },
    ]
}

fn tally(matched: usize, total: usize) -> Tally {
    Tally { total, matched }
}

#[test]
fn counts_a_variant_whose_bindings_are_exactly_its_modules_resources() {
    let fragment = vec![
        resource(0, 2, ResourceClass::Image, &[]),
        resource(0, 3, ResourceClass::Sampler, &[]),
        block(&[0, 16]),
    ];
    let mut report = ParameterReport::default();
    report.add_variant(&parameters(), &modules(fragment));
    assert_eq!(report.variants, 1);
    assert_eq!(report.stage_modules, tally(2, 2));
    assert_eq!(report.module_resources, tally(4, 4));
    assert_eq!(report.bindings["texture"], tally(1, 1));
    assert_eq!(report.bindings["constant buffer"], tally(1, 1));
    assert_eq!(report.sampler_slots, tally(1, 1));
    assert_eq!(report.buffer_layouts, tally(1, 1));
    assert_eq!(report.buffer_members, tally(2, 2));
}

#[test]
fn counts_what_the_modules_and_the_parameters_do_not_share() {
    // The fragment module lacks the sampler and has an image nothing binds, and neither
    // stage's block has the layout's second member.
    let fragment = vec![
        resource(0, 5, ResourceClass::Image, &[]),
        resource(0, 2, ResourceClass::Image, &[]),
        block(&[0]),
    ];
    let mut stages = modules(fragment);
    stages[0].resources = vec![block(&[0])];
    let mut report = ParameterReport::default();
    report.add_variant(&parameters(), &stages);
    assert_eq!(report.stage_modules, tally(1, 2));
    assert_eq!(report.module_resources, tally(3, 4));
    assert_eq!(report.bindings["texture"], tally(1, 1));
    assert_eq!(report.bindings["constant buffer"], tally(1, 1));
    assert_eq!(report.sampler_slots, tally(0, 1));
    assert_eq!(report.buffer_members, tally(1, 2));
}

#[test]
fn a_binding_naming_a_stage_with_no_module_does_not_land() {
    let mut report = ParameterReport::default();
    let vertex_only = vec![StageModule {
        slot: 0,
        resources: vec![block(&[0, 16])],
    }];
    report.add_variant(&parameters(), &vertex_only);
    assert_eq!(report.bindings["texture"], tally(0, 1));
    assert_eq!(report.bindings["constant buffer"], tally(0, 1));
}
