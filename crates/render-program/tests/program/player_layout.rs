//! Layouts and targets from a player variant's parameters: typed bindings with descriptor
//! slots, texture dimension codes and stage masks.

use wgpu::{BindingType, ShaderStages, TextureViewDimension};
use worldrunner_render_program::layout::UnusedDeclaration;
use worldrunner_render_program::unity::buffers::ParamName;
use worldrunner_render_program::unity::layout::{LayoutError, ProgramLayout};
use worldrunner_render_program::unity::module::{ResourceBinding, StageKind, StageProgram};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceKind, VariantParameters,
};

use crate::common::{player_binding, same, slot, split_table, stage};
use crate::fixtures::textured;

const VERTEX: u8 = DescriptorSlot::VERTEX;
const FRAGMENT: u8 = DescriptorSlot::FRAGMENT;
const TINTED: &str = "texel * material.tint";

fn texture_kind(dimension: i32, sampler: Option<DescriptorSlot>) -> ResourceKind {
    ResourceKind::Texture { sampler, dimension }
}

fn sampler_kind() -> ResourceKind {
    ResourceKind::Sampler { state: 0 }
}

fn parameters(bindings: Vec<(&str, DescriptorSlot, ResourceKind)>) -> VariantParameters {
    VariantParameters {
        constant_buffers: Vec::new(),
        bindings: bindings
            .into_iter()
            .map(|(name, slot, kind)| player_binding(name, slot, kind))
            .collect(),
    }
}

fn sampled(texture_type: &str, coordinates: &str) -> String {
    format!(
        "@group(0) @binding(0) var t: {texture_type};
         @group(0) @binding(1) var s: sampler;
         @fragment fn fs_main() -> @location(0) vec4<f32> {{
             return textureSample(t, s, {coordinates});
         }}"
    )
}

fn fragment(
    wgsl: &str,
    table: Vec<ResourceBinding>,
    parameters: VariantParameters,
) -> StageProgram {
    stage(wgsl, ("fs_main", StageKind::Fragment), table, parameters)
}

fn vertex(table: Vec<ResourceBinding>, parameters: VariantParameters) -> StageProgram {
    stage(
        &textured(TINTED),
        ("vs_main", StageKind::Vertex),
        table,
        parameters,
    )
}

/// The texture at binding 0 and sampler at binding 1 of a `sampled` module.
fn texture_pair(dimension: i32) -> VariantParameters {
    parameters(vec![
        ("t", slot(0, 0, FRAGMENT), texture_kind(dimension, None)),
        ("s", slot(0, 1, FRAGMENT), sampler_kind()),
    ])
}

fn pair_table() -> Vec<ResourceBinding> {
    vec![same(0, 0), same(0, 1)]
}

fn view_dimension(layout: &ProgramLayout) -> TextureViewDimension {
    match layout.slot(0, 0).unwrap().ty {
        BindingType::Texture { view_dimension, .. } => view_dimension,
        other => panic!("not a texture: {other:?}"),
    }
}

#[test]
fn dimension_codes_select_the_view_dimension_as_measured() {
    let cases = [
        (
            4,
            "texture_2d<f32>",
            "vec2<f32>(0.5)",
            TextureViewDimension::D2,
        ),
        (
            6,
            "texture_3d<f32>",
            "vec3<f32>(0.5)",
            TextureViewDimension::D3,
        ),
        (
            8,
            "texture_cube<f32>",
            "vec3<f32>(0.5)",
            TextureViewDimension::Cube,
        ),
        (
            10,
            "texture_2d_array<f32>",
            "vec2<f32>(0.5), 0",
            TextureViewDimension::D2Array,
        ),
    ];
    for (code, wgsl_type, coordinates, expected) in cases {
        let stage = fragment(
            &sampled(wgsl_type, coordinates),
            pair_table(),
            texture_pair(code),
        );
        let layout = ProgramLayout::build(&[&stage]).unwrap();
        assert_eq!(view_dimension(&layout), expected, "code {code}");
    }
}

#[test]
fn a_code_for_another_dimension_than_the_module_has_is_a_mismatch() {
    let cube_module = sampled("texture_cube<f32>", "vec3<f32>(0.5)");
    let stage = fragment(&cube_module, pair_table(), texture_pair(4));
    assert!(matches!(
        ProgramLayout::build(&[&stage]).unwrap_err(),
        LayoutError::Mismatch {
            set: 0,
            binding: 0,
            declared: "a texture",
            ..
        }
    ));
}

#[test]
fn dimension_codes_not_measured_are_named_errors() {
    let module = sampled("texture_2d<f32>", "vec2<f32>(0.5)");
    let build = |parameters| {
        let stage = fragment(&module, pair_table(), parameters);
        ProgramLayout::build(&[&stage]).unwrap_err()
    };
    for code in [5, 11] {
        assert_eq!(
            build(texture_pair(code)),
            LayoutError::MultisampledTexture { code },
            "code {code}"
        );
    }
    assert_eq!(
        build(texture_pair(7)),
        LayoutError::UnsupportedParameter {
            kind: "texture dimension 7".to_owned()
        }
    );
}

fn slot_types(layout: &ProgramLayout) -> Vec<(u32, u32, bool)> {
    layout
        .slots()
        .iter()
        .map(|s| (s.set, s.binding, matches!(s.ty, BindingType::Sampler(_))))
        .collect()
}

#[test]
fn a_texture_binding_with_a_sampler_slot_declares_the_split_sampler() {
    // The module has the combined image sampler at Unity binding 1; the split gave its
    // sampler binding 2. The texture names the combined binding as its own sampler slot, or
    // names no sampler, and either way the prepared sampler is declared.
    let module = textured(TINTED);
    let buffer = ("m", slot(0, 0, FRAGMENT), ResourceKind::ConstantBuffer);
    let own_slot = parameters(vec![
        buffer,
        (
            "albedo",
            slot(0, 1, FRAGMENT),
            texture_kind(4, Some(slot(0, 1, FRAGMENT))),
        ),
    ]);
    let none = parameters(vec![
        buffer,
        ("albedo", slot(0, 1, FRAGMENT), texture_kind(4, None)),
    ]);
    let layouts: Vec<_> = [own_slot, none]
        .into_iter()
        .map(|parameters| {
            let stage = stage(
                &module,
                ("fs_main", StageKind::Fragment),
                split_table(),
                parameters,
            );
            ProgramLayout::build(&[&stage]).unwrap()
        })
        .collect();
    assert_eq!(layouts[0], layouts[1]);
    assert_eq!(
        slot_types(&layouts[0]),
        [(0, 0, false), (0, 1, false), (0, 2, true)]
    );
}

#[test]
fn a_texture_binding_names_its_separate_sampler_by_slot() {
    let module = textured(TINTED);
    let parameters = parameters(vec![
        ("m", slot(0, 0, FRAGMENT), ResourceKind::ConstantBuffer),
        (
            "albedo",
            slot(0, 1, FRAGMENT),
            texture_kind(4, Some(slot(0, 2, FRAGMENT))),
        ),
    ]);
    let table = (0..4).map(|b| same(0, b)).collect();
    let stage = stage(&module, ("fs_main", StageKind::Fragment), table, parameters);
    let layout = ProgramLayout::build(&[&stage]).unwrap();
    assert_eq!(
        slot_types(&layout),
        [(0, 0, false), (0, 1, false), (0, 2, true)]
    );
}

fn identity_table() -> Vec<ResourceBinding> {
    (0..4).map(|b| same(0, b)).collect()
}

#[test]
fn a_binding_that_does_not_name_the_modules_stage_is_a_stage_mismatch_when_the_module_uses_it() {
    // The fragment module uses the texture, but the parameters bind it for the vertex stage.
    let parameters = parameters(vec![
        ("m", slot(0, 0, FRAGMENT), ResourceKind::ConstantBuffer),
        ("albedo", slot(0, 1, VERTEX), texture_kind(4, None)),
        ("s", slot(0, 2, FRAGMENT), sampler_kind()),
    ]);
    let stage = fragment(&textured(TINTED), identity_table(), parameters);
    assert_eq!(
        ProgramLayout::build(&[&stage]).unwrap_err(),
        LayoutError::StageMismatch {
            set: 0,
            binding: 1,
            stage: StageKind::Fragment,
            stages: VERTEX
        }
    );
    // The vertex module uses constants the parameters bind for the fragment stage only.
    let parameters = parameters_for_vertex(FRAGMENT, VERTEX);
    assert_eq!(
        ProgramLayout::build(&[&vertex(identity_table(), parameters)]).unwrap_err(),
        LayoutError::StageMismatch {
            set: 0,
            binding: 0,
            stage: StageKind::Vertex,
            stages: FRAGMENT
        }
    );
}

fn parameters_for_vertex(material: u8, draw: u8) -> VariantParameters {
    parameters(vec![
        ("m", slot(0, 0, material), ResourceKind::ConstantBuffer),
        ("d", slot(0, 3, draw), ResourceKind::ConstantBuffer),
    ])
}

#[test]
fn visibility_comes_from_the_mask_and_includes_the_modules_stage() {
    let both = VERTEX | FRAGMENT;
    let stage = vertex(identity_table(), parameters_for_vertex(both, VERTEX));
    let layout = ProgramLayout::build(&[&stage]).unwrap();
    let visibility: Vec<_> = layout
        .slots()
        .iter()
        .map(|s| (s.binding, s.visibility))
        .collect();
    assert_eq!(
        visibility,
        [
            (0, ShaderStages::VERTEX | ShaderStages::FRAGMENT),
            (3, ShaderStages::VERTEX)
        ]
    );
}

#[test]
fn bindings_that_name_only_other_stages_are_neither_bound_nor_reported_nor_checked() {
    // The vertex stage's parameters also describe the fragment stage: a texture and sampler
    // the vertex table does not hold, and a storage image this renderer does not bind.
    let parameters = parameters(vec![
        (
            "m",
            slot(0, 0, VERTEX | FRAGMENT),
            ResourceKind::ConstantBuffer,
        ),
        ("d", slot(0, 3, VERTEX), ResourceKind::ConstantBuffer),
        ("albedo", slot(0, 8, FRAGMENT), texture_kind(4, None)),
        ("s", slot(0, 9, FRAGMENT), sampler_kind()),
        ("img", slot(0, 10, FRAGMENT), ResourceKind::StorageImage),
    ]);
    let layout = ProgramLayout::build(&[&vertex(identity_table(), parameters)]).unwrap();
    assert_eq!(layout.slots().len(), 2);
    assert!(layout.unused().is_empty());
}

#[test]
fn a_binding_the_module_does_not_use_is_reported_with_its_stage_and_not_bound() {
    let parameters = parameters(vec![
        ("m", slot(0, 0, VERTEX), ResourceKind::ConstantBuffer),
        ("d", slot(0, 3, VERTEX), ResourceKind::ConstantBuffer),
        ("unused", slot(0, 1, VERTEX), ResourceKind::Buffer),
    ]);
    let layout = ProgramLayout::build(&[&vertex(identity_table(), parameters)]).unwrap();
    assert_eq!(layout.slots().len(), 2);
    assert_eq!(
        layout.unused(),
        [UnusedDeclaration {
            stage: StageKind::Vertex,
            set: 0,
            binding: 1
        }]
    );
}

#[test]
fn a_binding_outside_the_table_is_named_with_its_set_and_a_duplicate_is_ambiguous() {
    let parameters = || parameters_for_vertex(VERTEX, VERTEX);
    let missing = vec![same(0, 0), same(1, 3)];
    assert_eq!(
        ProgramLayout::build(&[&vertex(missing, parameters())]).unwrap_err(),
        LayoutError::UnboundSlot { set: 0, binding: 3 }
    );
    let twice = vec![same(0, 0), same(0, 3), same(0, 3)];
    assert_eq!(
        ProgramLayout::build(&[&vertex(twice, parameters())]).unwrap_err(),
        LayoutError::AmbiguousBinding { unity_binding: 3 }
    );
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

fn buffer_layout(name: &str, member_name: &str) -> ConstantBuffer {
    ConstantBuffer {
        name: name.to_owned(),
        size: 16,
        members: vec![member(member_name, 0)],
        structs: Vec::new(),
    }
}

#[test]
fn targets_name_each_buffer_and_texture_at_its_prepared_binding() {
    // Unity's draw constants at binding 2 are at prepared binding 3 after the split, and the
    // texture at 1 keeps its binding.
    let mut parameters = parameters(vec![
        (
            "Material",
            slot(0, 0, VERTEX | FRAGMENT),
            ResourceKind::ConstantBuffer,
        ),
        ("Draw", slot(0, 2, VERTEX), ResourceKind::ConstantBuffer),
        ("_MainTex", slot(0, 1, FRAGMENT), texture_kind(4, None)),
    ]);
    parameters.constant_buffers = vec![
        buffer_layout("", "ignored"),
        buffer_layout("Material", "_Tint"),
        buffer_layout("Draw", "_Scale"),
    ];
    let vertex_stage = vertex(split_table(), parameters.clone());
    let targets = vertex_stage.constant_buffer_targets().unwrap();
    let found: Vec<_> = targets
        .iter()
        .map(|t| (t.buffer.name.clone(), t.set, t.binding))
        .collect();
    assert_eq!(
        found,
        [
            (ParamName::Named("Material".to_owned()), 0, 0),
            (ParamName::Named("Draw".to_owned()), 0, 3)
        ]
    );
    assert_eq!(
        targets[1].buffer.members[0].name,
        ParamName::Named("_Scale".to_owned())
    );
    assert!(vertex_stage.texture_targets().unwrap().is_empty());

    let fragment_stage = fragment(&textured(TINTED), split_table(), parameters);
    let found: Vec<_> = fragment_stage
        .constant_buffer_targets()
        .unwrap()
        .iter()
        .map(|t| (t.set, t.binding))
        .collect();
    assert_eq!(found, [(0, 0)]);
    let textures: Vec<_> = fragment_stage
        .texture_targets()
        .unwrap()
        .into_iter()
        .map(|t| (t.name, t.set, t.binding))
        .collect();
    assert_eq!(textures, [(ParamName::Named("_MainTex".to_owned()), 0, 1)]);
}

#[test]
fn a_constant_buffer_binding_without_a_layout_of_its_name_is_a_named_error() {
    let parameters = parameters(vec![(
        "Material",
        slot(0, 0, VERTEX),
        ResourceKind::ConstantBuffer,
    )]);
    let stage = vertex(identity_table(), parameters);
    assert_eq!(
        stage.constant_buffer_targets().unwrap_err(),
        LayoutError::UndescribedBuffer {
            name: "Material".to_owned()
        }
    );
}
