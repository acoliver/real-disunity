//! The built-ins report counts, over the variants of a player, the constant buffer members that
//! are never a material property, and lists the ones most variants hold.

use std::collections::BTreeSet;

use worldrunner_unity_import::player_builtins::{BuiltinReport, MemberShape};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, StructLayout,
    VariantParameters,
};

fn member(name: &str, rows: i32, columns: i32, is_matrix: bool) -> BufferMember {
    BufferMember {
        name: name.to_owned(),
        kind: 0,
        rows,
        columns,
        is_matrix,
        array_size: 0,
        offset: 0,
    }
}

fn buffer(name: &str, members: Vec<BufferMember>) -> ConstantBuffer {
    ConstantBuffer {
        name: name.to_owned(),
        size: 0,
        members,
        structs: Vec::new(),
    }
}

fn variant(buffers: Vec<ConstantBuffer>) -> VariantParameters {
    VariantParameters {
        constant_buffers: buffers,
        bindings: Vec::new(),
    }
}

fn properties(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn shape(buffer: &str, name: &str, rows: i32, columns: i32, is_matrix: bool) -> MemberShape {
    MemberShape {
        buffer: buffer.to_owned(),
        name: name.to_owned(),
        kind: 0,
        rows,
        columns,
        is_matrix,
        array_size: 0,
    }
}

#[test]
fn a_member_counts_once_per_variant_and_once_more_when_the_shader_declares_it() {
    let mut report = BuiltinReport::default();
    report.add_variant(
        &variant(vec![buffer(
            "Draw",
            vec![member("m", 4, 4, true), member("m", 4, 4, true)],
        )]),
        &properties(&[]),
    );
    report.add_variant(
        &variant(vec![buffer("Draw", vec![member("m", 4, 4, true)])]),
        &properties(&["m"]),
    );
    let counts = report.members[&shape("Draw", "m", 4, 4, true)];
    assert_eq!(report.variants, 2);
    assert_eq!(counts.variants, 2, "twice in one variant is one variant");
    assert_eq!(counts.declared, 1);
}

#[test]
fn members_that_differ_in_shape_or_buffer_are_counted_apart() {
    let mut report = BuiltinReport::default();
    report.add_variant(
        &variant(vec![
            buffer("A", vec![member("v", 1, 4, false)]),
            buffer(
                "B",
                vec![member("v", 1, 4, false), member("v", 1, 1, false)],
            ),
        ]),
        &properties(&[]),
    );
    assert_eq!(report.members.len(), 3);
}

#[test]
fn struct_members_are_counted_under_the_struct() {
    let mut report = BuiltinReport::default();
    let mut holder = buffer("", Vec::new());
    holder.structs.push(StructLayout {
        name: "Instances".to_owned(),
        index: 0,
        array_size: 2,
        size: 64,
        members: vec![member("s", 4, 4, true)],
    });
    report.add_variant(&variant(vec![holder]), &properties(&[]));
    let key = shape("struct Instances", "s", 4, 4, true);
    assert_eq!(report.members[&key].variants, 1);
}

#[test]
fn the_listing_holds_only_members_never_declared_and_in_enough_variants() {
    let mut report = BuiltinReport::default();
    for index in 0..10 {
        let declared = if index < 2 {
            properties(&["tint"])
        } else {
            properties(&[])
        };
        let mut members = vec![
            member("everywhere", 1, 4, false),
            member("tint", 1, 4, false),
        ];
        if index < 3 {
            members.push(member("some", 1, 4, false));
        }
        report.add_variant(&variant(vec![buffer("Draw", members)]), &declared);
    }
    let listed: Vec<_> = report
        .listing(50)
        .into_iter()
        .map(|(shape, counts)| (shape.name, counts.variants))
        .collect();
    // `tint` is declared by two of ten variants' shaders, so it is a material property;
    // `some` is in three of ten, below the half the listing asks for.
    assert_eq!(listed, [("everywhere".to_owned(), 10)]);
}

#[test]
fn the_listing_runs_from_the_most_common_member_down() {
    let mut report = BuiltinReport::default();
    for index in 0..4 {
        let mut members = vec![member("b", 1, 4, false)];
        if index < 3 {
            members.push(member("a", 1, 4, false));
        }
        report.add_variant(&variant(vec![buffer("Draw", members)]), &properties(&[]));
    }
    let names: Vec<_> = report
        .listing(50)
        .into_iter()
        .map(|(shape, _)| shape.name)
        .collect();
    assert_eq!(names, ["b", "a"]);
}

fn texture_binding(name: &str) -> ResourceBinding {
    ResourceBinding {
        name: name.to_owned(),
        slot: DescriptorSlot {
            binding: 0,
            set: 0,
            stages: DescriptorSlot::FRAGMENT,
        },
        kind: ResourceKind::Texture {
            sampler: None,
            dimension: 4,
        },
    }
}

#[test]
fn a_texture_counts_with_its_scale_offset_and_texel_size_members_when_the_variant_has_them() {
    let mut report = BuiltinReport::default();
    let mut parameters = variant(vec![buffer(
        "Material",
        vec![
            member("_Main_ST", 1, 4, false),
            member("_Main_TexelSize", 1, 4, false),
            member("_Other_ST", 4, 4, true),
        ],
    )]);
    parameters.bindings = vec![
        texture_binding("_Main"),
        texture_binding("_Other"),
        texture_binding("_Bare"),
    ];
    report.add_variant(&parameters, &properties(&["_Main", "_Other", "_Bare"]));
    assert_eq!(report.textures, 3);
    assert_eq!(
        report.with_transform, 1,
        "a matrix named _ST is not a vector"
    );
    assert_eq!(report.with_texel_size, 1);
}

#[test]
fn buffers_that_differ_only_in_a_trailing_hash_are_one_buffer() {
    let mut report = BuiltinReport::default();
    for name in ["Globals1067298408", "Globals118734948"] {
        report.add_variant(
            &variant(vec![buffer(name, vec![member("m", 4, 4, true)])]),
            &properties(&[]),
        );
    }
    report.add_variant(
        &variant(vec![buffer("Light2", vec![member("m", 4, 4, true)])]),
        &properties(&[]),
    );
    let listed: Vec<_> = report
        .listing(0)
        .into_iter()
        .map(|(shape, counts)| (shape.buffer, counts.variants))
        .collect();
    assert_eq!(
        listed,
        [("Globals#".to_owned(), 2), ("Light2".to_owned(), 1)],
        "ten digits are a hash; one digit is part of a name"
    );
}
