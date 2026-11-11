use worldrunner_unity_import::player_layouts::{LayoutReport, RawStage};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, StructLayout,
    VariantParameters,
};

const VERTEX: u8 = DescriptorSlot::VERTEX;
const FRAGMENT: u8 = DescriptorSlot::FRAGMENT;

fn op(code: u32, operands: &[u32]) -> Vec<u32> {
    let len = u32::try_from(operands.len() + 1).unwrap();
    [vec![(len << 16) | code], operands.to_vec()].concat()
}

/// A vertex module whose position is the first member of a uniform block at set 0 binding 0.
fn vertex_module() -> Vec<u32> {
    vertex_module_at(0)
}

/// A vertex module whose position is the first member of a uniform block at set 0 and
/// `binding`.
/// IDs: 1 main, 2 position output, 3 block variable, 4 void, 5 function type, 6 float,
/// 7 vec4, 8 block, 9 block pointer, 10 output pointer, 11 int, 12 zero, 13 member pointer,
/// 14 label, 15 member access, 16 loaded vector.
fn vertex_module_at(binding: u32) -> Vec<u32> {
    let main_name = 0x6e69_616d;
    [
        vec![0x0723_0203, 0x0001_0000, 0, 17, 0],
        op(17, &[1]),
        op(14, &[0, 1]),
        op(15, &[0, 1, main_name, 0, 2]),
        op(71, &[2, 11, 0]),
        op(71, &[8, 2]),
        op(72, &[8, 0, 35, 0]),
        op(71, &[3, 34, 0]),
        op(71, &[3, 33, binding]),
        op(19, &[4]),
        op(33, &[5, 4]),
        op(22, &[6, 32]),
        op(23, &[7, 6, 4]),
        op(30, &[8, 7]),
        op(32, &[9, 2, 8]),
        op(59, &[9, 3, 2]),
        op(32, &[10, 3, 7]),
        op(59, &[10, 2, 3]),
        op(21, &[11, 32, 1]),
        op(43, &[11, 12, 0]),
        op(32, &[13, 2, 7]),
        op(54, &[4, 1, 0, 5]),
        op(248, &[14]),
        op(65, &[13, 15, 3, 12]),
        op(61, &[7, 16, 15]),
        op(62, &[2, 16]),
        op(253, &[]),
        op(56, &[]),
    ]
    .concat()
}

fn vertex_stage() -> RawStage {
    RawStage {
        slot: 0,
        words: vertex_module(),
    }
}

fn slot(stages: u8) -> DescriptorSlot {
    DescriptorSlot {
        binding: 0,
        set: 0,
        stages,
    }
}

fn block_binding(stages: u8) -> ResourceBinding {
    ResourceBinding {
        name: "Block".to_owned(),
        slot: slot(stages),
        kind: ResourceKind::ConstantBuffer,
    }
}

fn parameters(bindings: Vec<ResourceBinding>) -> VariantParameters {
    VariantParameters {
        constant_buffers: vec![ConstantBuffer {
            name: "Block".to_owned(),
            size: 16,
            members: vec![BufferMember {
                name: "position".to_owned(),
                kind: 0,
                rows: 1,
                columns: 4,
                is_matrix: false,
                array_size: 0,
                offset: 0,
            }],
            structs: Vec::new(),
        }],
        bindings,
    }
}

fn report_of(parameters: &VariantParameters, stages: &[RawStage]) -> LayoutReport {
    let mut report = LayoutReport::default();
    report.add_variant(parameters, stages);
    report
}

#[test]
fn a_variant_whose_parameters_describe_its_module_builds_a_layout() {
    let report = report_of(&parameters(vec![block_binding(VERTEX)]), &[vertex_stage()]);
    assert_eq!((report.variants, report.layouts), (1, 1));
    assert_eq!(report.stage_modules, 1);
    assert_eq!(report.prepared, 1);
    assert_eq!(report.stage_programs, 1);
    assert_eq!(report.unused_parameters, 0);
    assert_eq!((report.targets.total, report.targets.matched), (1, 1));
    assert!(report.layout_failures.is_empty());
    assert_eq!(
        report.member_shapes.iter().collect::<Vec<_>>(),
        [(&"kind 0 1x4: packs".to_owned(), &1)]
    );
}

#[test]
fn a_binding_outside_the_modules_resources_fails_the_layout_by_a_named_cause() {
    let extra = ResourceBinding {
        name: "Other".to_owned(),
        slot: DescriptorSlot {
            binding: 5,
            ..slot(VERTEX)
        },
        kind: ResourceKind::Buffer,
    };
    let report = report_of(
        &parameters(vec![block_binding(VERTEX), extra]),
        &[vertex_stage()],
    );
    assert_eq!(report.layouts, 0);
    assert_eq!(
        report.layout_failures.keys().collect::<Vec<_>>(),
        ["binding outside the module's table"]
    );
}

#[test]
fn a_binding_naming_other_stages_fails_the_layout_by_a_named_cause() {
    let report = report_of(
        &parameters(vec![block_binding(FRAGMENT)]),
        &[vertex_stage()],
    );
    assert_eq!((report.variants, report.layouts), (1, 0));
    assert_eq!(
        report.layout_failures.iter().collect::<Vec<_>>(),
        [(&"stage mismatch".to_owned(), &1)]
    );
}

#[test]
fn an_unsupported_binding_fails_the_layout_by_a_named_cause() {
    let multisampled = ResourceBinding {
        name: "Image".to_owned(),
        slot: DescriptorSlot {
            binding: 4,
            ..slot(VERTEX)
        },
        kind: ResourceKind::Texture {
            sampler: None,
            dimension: 5,
        },
    };
    let report = report_of(
        &parameters(vec![block_binding(VERTEX), multisampled]),
        &[vertex_stage()],
    );
    assert_eq!(report.layouts, 0);
    assert_eq!(
        report.layout_failures.keys().collect::<Vec<_>>(),
        ["multisampled texture, code 5"]
    );
}

#[test]
fn a_module_that_cannot_be_prepared_or_parsed_is_counted_by_cause() {
    let garbage = RawStage {
        slot: 0,
        words: vec![1, 2, 3],
    };
    let report = report_of(&parameters(vec![block_binding(VERTEX)]), &[garbage]);
    assert_eq!(
        (report.stage_modules, report.prepared, report.layouts),
        (1, 0, 0)
    );
    assert_eq!(report.prepare_failures.values().sum::<usize>(), 1);
    let other_stage = RawStage {
        slot: 3,
        ..vertex_stage()
    };
    let report = report_of(&parameters(vec![block_binding(VERTEX)]), &[other_stage]);
    assert_eq!(report.layouts, 0);
    assert_eq!(
        report.stage_failures.keys().collect::<Vec<_>>(),
        ["a stage other than vertex or fragment"]
    );
}

/// A fragment module that samples the texture at set 0 binding 1 with the sampler at binding 2.
/// IDs: 1 main, 2 color output, 3 texture, 4 sampler, 5 void, 6 function type, 7 float,
/// 8 vec4, 9 vec2, 10 image, 11 sampler type, 12 sampled image, 13 to 15 pointers, 16 half,
/// 17 coordinate, 18 label, 19 loaded image, 20 loaded sampler, 21 combined, 22 color.
fn fragment_module() -> Vec<u32> {
    let main_name = 0x6e69_616d;
    [
        vec![0x0723_0203, 0x0001_0000, 0, 23, 0],
        op(17, &[1]),
        op(14, &[0, 1]),
        op(15, &[4, 1, main_name, 0, 2]),
        op(16, &[1, 7]),
        op(71, &[2, 30, 0]),
        op(71, &[3, 34, 0]),
        op(71, &[3, 33, 1]),
        op(71, &[4, 34, 0]),
        op(71, &[4, 33, 2]),
        op(19, &[5]),
        op(33, &[6, 5]),
        op(22, &[7, 32]),
        op(23, &[8, 7, 4]),
        op(23, &[9, 7, 2]),
        op(25, &[10, 7, 1, 0, 0, 0, 1, 0]),
        op(26, &[11]),
        op(27, &[12, 10]),
        op(32, &[13, 0, 10]),
        op(32, &[14, 0, 11]),
        op(32, &[15, 3, 8]),
        op(59, &[13, 3, 0]),
        op(59, &[14, 4, 0]),
        op(59, &[15, 2, 3]),
        op(43, &[7, 16, 0x3f00_0000]),
        op(44, &[9, 17, 16, 16]),
        op(54, &[5, 1, 0, 6]),
        op(248, &[18]),
        op(61, &[10, 19, 3]),
        op(61, &[11, 20, 4]),
        op(86, &[12, 21, 19, 20]),
        op(87, &[8, 22, 21, 17]),
        op(62, &[2, 22]),
        op(253, &[]),
        op(56, &[]),
    ]
    .concat()
}

fn fragment_stage() -> RawStage {
    RawStage {
        slot: 1,
        words: fragment_module(),
    }
}

fn textured(dimension: i32) -> VariantParameters {
    let at = |binding| DescriptorSlot {
        binding,
        set: 0,
        stages: FRAGMENT,
    };
    VariantParameters {
        constant_buffers: Vec::new(),
        bindings: vec![
            ResourceBinding {
                name: "Tex".to_owned(),
                slot: at(1),
                kind: ResourceKind::Texture {
                    sampler: Some(at(2)),
                    dimension,
                },
            },
            ResourceBinding {
                name: String::new(),
                slot: at(2),
                kind: ResourceKind::Sampler { state: 0 },
            },
        ],
    }
}

#[test]
fn texture_dimension_codes_are_tallied_against_the_modules_image() {
    let report = report_of(&textured(4), &[fragment_stage()]);
    assert_eq!(report.layouts, 1, "{report}");
    assert_eq!(
        report.texture_codes.iter().collect::<Vec<_>>(),
        [(&"code 4: D2".to_owned(), &1)]
    );
    // A code that is not measured still has its module's image counted, which is how it gets
    // measured.
    let report = report_of(&textured(5), &[fragment_stage()]);
    assert_eq!(report.layouts, 0);
    assert_eq!(
        report.layout_failures.keys().collect::<Vec<_>>(),
        ["multisampled texture, code 5"]
    );
    assert_eq!(
        report.texture_codes.iter().collect::<Vec<_>>(),
        [(&"code 5: D2".to_owned(), &1)]
    );
}

#[test]
fn a_vertex_and_a_fragment_stage_build_one_layout() {
    let parameters = VariantParameters {
        constant_buffers: parameters(vec![]).constant_buffers,
        bindings: [vec![block_binding(VERTEX)], textured(4).bindings].concat(),
    };
    let report = report_of(&parameters, &[vertex_stage(), fragment_stage()]);
    assert_eq!((report.variants, report.layouts), (1, 1), "{report}");
    assert_eq!(report.stage_programs, 2);
    assert_eq!(
        (
            report.agreeing_numbering.total,
            report.agreeing_numbering.matched
        ),
        (1, 1)
    );
    assert_eq!(report.conflicts_despite_agreement, 0);
}

#[test]
fn members_the_packer_refuses_are_counted_by_shape() {
    let mut parameters = parameters(vec![block_binding(VERTEX)]);
    let member = |kind, rows, columns, is_matrix, array_size| BufferMember {
        name: "m".to_owned(),
        kind,
        rows,
        columns,
        is_matrix,
        array_size,
        offset: 0,
    };
    parameters.constant_buffers[0].members = vec![
        member(0, 4, 4, true, 0),
        member(1, 1, 3, false, 5),
        member(2, 1, 1, false, 0),
    ];
    let report = report_of(&parameters, &[vertex_stage()]);
    assert_eq!(
        report.member_shapes.iter().collect::<Vec<_>>(),
        [
            (&"kind 0 4x4 matrix: packs".to_owned(), &1),
            (&"kind 1 1x3 array: refused".to_owned(), &1),
            (&"kind 2 1x1: refused".to_owned(), &1),
        ]
    );
}

#[test]
fn a_buffer_with_an_array_of_structs_is_counted_as_packing_with_its_members_by_shape() {
    let mut parameters = parameters(vec![block_binding(VERTEX)]);
    let member = |kind, columns, offset| BufferMember {
        name: "m".to_owned(),
        kind,
        rows: 1,
        columns,
        is_matrix: false,
        array_size: 0,
        offset,
    };
    // The player's buffers are as large as their struct array: 2 elements of 32 bytes.
    parameters.constant_buffers[0].size = 64;
    parameters.constant_buffers[0].members.clear();
    parameters.constant_buffers[0].structs = vec![StructLayout {
        name: "S".to_owned(),
        index: 0,
        array_size: 2,
        size: 32,
        members: vec![member(0, 4, 0), member(1, 4, 16)],
    }];
    let report = report_of(&parameters, &[vertex_stage()]);
    assert_eq!(
        report.member_shapes.iter().collect::<Vec<_>>(),
        [
            (&"buffer with structs: packs".to_owned(), &1),
            (&"struct member kind 0 1x4: packs".to_owned(), &1),
            (&"struct member kind 1 1x4: packs".to_owned(), &1),
        ]
    );
    // A member the packer refuses refuses the buffer, and is counted as refused by its shape.
    parameters.constant_buffers[0].structs[0]
        .members
        .push(member(2, 1, 0));
    let report = report_of(&parameters, &[vertex_stage()]);
    assert_eq!(
        report.member_shapes.iter().collect::<Vec<_>>(),
        [
            (&"buffer with structs: refused".to_owned(), &1),
            (&"struct member kind 0 1x4: packs".to_owned(), &1),
            (&"struct member kind 1 1x4: packs".to_owned(), &1),
            (&"struct member kind 2 1x1: refused".to_owned(), &1),
        ]
    );
}

/// A fragment module that samples a combined image sampler at set 0 binding 1. IDs: 1 main,
/// 2 color output, 3 combined sampler, 5 void, 6 function type, 7 float, 8 vec4, 9 vec2,
/// 10 image, 12 sampled image, 13 and 15 pointers, 16 half, 17 coordinate, 18 label, 21 loaded
/// combined sampler, 22 color.
fn combined_fragment_module() -> Vec<u32> {
    let main_name = 0x6e69_616d;
    [
        vec![0x0723_0203, 0x0001_0000, 0, 23, 0],
        op(17, &[1]),
        op(14, &[0, 1]),
        op(15, &[4, 1, main_name, 0, 2]),
        op(16, &[1, 7]),
        op(71, &[2, 30, 0]),
        op(71, &[3, 34, 0]),
        op(71, &[3, 33, 1]),
        op(19, &[5]),
        op(33, &[6, 5]),
        op(22, &[7, 32]),
        op(23, &[8, 7, 4]),
        op(23, &[9, 7, 2]),
        op(25, &[10, 7, 1, 0, 0, 0, 1, 0]),
        op(27, &[12, 10]),
        op(32, &[13, 0, 12]),
        op(32, &[15, 3, 8]),
        op(59, &[13, 3, 0]),
        op(59, &[15, 2, 3]),
        op(43, &[7, 16, 0x3f00_0000]),
        op(44, &[9, 17, 16, 16]),
        op(54, &[5, 1, 0, 6]),
        op(248, &[18]),
        op(61, &[12, 21, 3]),
        op(87, &[8, 22, 21, 17]),
        op(62, &[2, 22]),
        op(253, &[]),
        op(56, &[]),
    ]
    .concat()
}

#[test]
fn stages_that_split_different_samplers_are_prepared_together_and_lay_out() {
    // The uniform block is at binding 2 in the vertex stage, which splits nothing. The
    // fragment stage splits the combined sampler at binding 1, so the sampler takes binding 2
    // there and the block must move: prepared alone, the two stages would put a sampler and a
    // uniform buffer at one prepared binding.
    let block = ResourceBinding {
        slot: DescriptorSlot {
            binding: 2,
            ..slot(VERTEX)
        },
        ..block_binding(VERTEX)
    };
    let texture = ResourceBinding {
        name: "Tex".to_owned(),
        slot: DescriptorSlot {
            binding: 1,
            ..slot(FRAGMENT)
        },
        kind: ResourceKind::Texture {
            sampler: None,
            dimension: 4,
        },
    };
    let parameters = VariantParameters {
        bindings: vec![block, texture],
        ..parameters(Vec::new())
    };
    let stages = [
        RawStage {
            slot: 0,
            words: vertex_module_at(2),
        },
        RawStage {
            slot: 1,
            words: combined_fragment_module(),
        },
    ];
    let report = report_of(&parameters, &stages);
    assert_eq!((report.variants, report.layouts), (1, 1), "{report}");
    assert_eq!((report.stage_modules, report.prepared), (2, 2));
    assert!(report.prepare_failures.is_empty() && report.layout_failures.is_empty());
    assert_eq!(
        (
            report.agreeing_numbering.total,
            report.agreeing_numbering.matched
        ),
        (1, 1)
    );
    assert_eq!(report.conflicts_despite_agreement, 0);
}

#[test]
fn stages_that_declare_one_binding_as_different_resources_fail_preparation_once() {
    // A vertex uniform block and a fragment combined sampler, both at binding 1.
    let stages = [
        RawStage {
            slot: 0,
            words: vertex_module_at(1),
        },
        RawStage {
            slot: 1,
            words: combined_fragment_module(),
        },
    ];
    let report = report_of(&parameters(Vec::new()), &stages);
    assert_eq!(
        (report.stage_modules, report.prepared, report.layouts),
        (2, 0, 0)
    );
    assert_eq!(
        report.prepare_failures.iter().collect::<Vec<_>>(),
        [(&"binding class".to_owned(), &1)]
    );
}
