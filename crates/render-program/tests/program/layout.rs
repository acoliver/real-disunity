//! Bind group layouts from Unity parameters, binding tables and the module's own resources,
//! with the parameters described as an editor-style sub program and as a player variant.

use wgpu::{BindingType, BufferBindingType, SamplerBindingType, ShaderStages};
use wgpu::{TextureSampleType, TextureViewDimension};
use worldrunner_render_program::layout::{LayoutError, ProgramLayout};
use worldrunner_render_program::module::{ResourceBinding, StageKind, StageProgram};

use crate::common::{
    Dim, FORMS, Form, Resources, parameters, same, sampler, split_table, stage, stage_in, texture,
};
use crate::fixtures::textured;

/// A fragment color that reads the material constants, like the vertex stage does.
const TINTED: &str = "texel * material.tint";
const SHADER: &str = "
@group(0) @binding(0) var<uniform> m: vec4<f32>;
@vertex fn vs_main() -> @builtin(position) vec4<f32> { return m; }";

const VERTEX: StageKind = StageKind::Vertex;
const FRAGMENT: StageKind = StageKind::Fragment;

fn identity_table() -> Vec<ResourceBinding> {
    (0..4).map(|b| same(0, b)).collect()
}

fn vertex_resources() -> Resources {
    Resources::new(&[0, 3], &[], &[])
}

fn fragment_resources() -> Resources {
    Resources::new(&[0], &[(1, Dim::D2)], &[2])
}

/// The textured pass's vertex and fragment stages over `table`.
fn pass_stages(form: Form, table: &[ResourceBinding]) -> [StageProgram; 2] {
    let (vertex, fragment) = (vertex_resources(), fragment_resources());
    let pass = [(VERTEX, &vertex), (FRAGMENT, &fragment)];
    [
        stage_in(
            form,
            &textured(TINTED),
            ("vs_main", VERTEX),
            table.to_vec(),
            &pass,
        ),
        stage_in(
            form,
            &textured(TINTED),
            ("fs_main", FRAGMENT),
            table.to_vec(),
            &pass,
        ),
    ]
}

/// A fragment stage over `table` that binds `fragment` and nothing else.
fn fragment_only(form: Form, table: Vec<ResourceBinding>, fragment: &Resources) -> StageProgram {
    stage_in(
        form,
        &textured(TINTED),
        ("fs_main", FRAGMENT),
        table,
        &[(FRAGMENT, fragment)],
    )
}

fn build(stages: &[StageProgram]) -> Result<ProgramLayout, LayoutError> {
    let stages: Vec<&StageProgram> = stages.iter().collect();
    ProgramLayout::build(&stages)
}

fn uniform() -> BindingType {
    BindingType::Buffer {
        ty: BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

#[test]
fn stages_are_merged_with_the_union_of_their_visibility() {
    for form in FORMS {
        let layout = build(&pass_stages(form, &identity_table())).unwrap();
        let slots: Vec<_> = layout
            .slots()
            .iter()
            .map(|s| (s.set, s.binding, s.visibility))
            .collect();
        assert_eq!(
            slots,
            [
                (0, 0, ShaderStages::VERTEX | ShaderStages::FRAGMENT),
                (0, 1, ShaderStages::FRAGMENT),
                (0, 2, ShaderStages::FRAGMENT),
                (0, 3, ShaderStages::VERTEX),
            ],
            "{form:?}"
        );
        assert_eq!(layout.slot(0, 0).unwrap().ty, uniform());
        assert_eq!(layout.slot(0, 0).unwrap().min_size, 32, "two vec4s");
        assert_eq!(layout.slot(0, 3).unwrap().min_size, 16);
        assert_eq!(
            layout.slot(0, 1).unwrap().ty,
            BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            }
        );
        assert_eq!(
            layout.slot(0, 2).unwrap().ty,
            BindingType::Sampler(SamplerBindingType::Filtering)
        );
        assert!(layout.unused().is_empty());
    }
}

#[test]
fn a_split_combined_sampler_binds_the_sampler_the_table_names() {
    for form in FORMS {
        let vertex = Resources::new(&[0, 2], &[], &[]);
        let fragment = Resources::new(&[0], &[(1, Dim::D2)], &[]);
        let pass = [(VERTEX, &vertex), (FRAGMENT, &fragment)];
        let table = split_table();
        let split = build(&[
            stage_in(
                form,
                &textured(TINTED),
                ("vs_main", VERTEX),
                table.clone(),
                &pass,
            ),
            stage_in(form, &textured(TINTED), ("fs_main", FRAGMENT), table, &pass),
        ])
        .unwrap();
        let plain = build(&pass_stages(form, &identity_table())).unwrap();
        assert_eq!(split, plain, "{form:?}");
    }
}

#[test]
fn a_parameter_the_entry_point_does_not_use_is_reported_not_bound() {
    for form in FORMS {
        let extra = Resources::new(&[0, 1, 3], &[], &[]);
        let vertex = stage_in(
            form,
            &textured(TINTED),
            ("vs_main", VERTEX),
            identity_table(),
            &[(VERTEX, &extra)],
        );
        let layout = build(&[vertex]).unwrap();
        assert_eq!(layout.slots().len(), 2, "{form:?}");
        let unused: Vec<_> = layout.unused().iter().map(|u| (u.set, u.binding)).collect();
        assert_eq!(unused, [(0, 1)], "{form:?}");
    }
}

#[test]
fn unsupported_editor_parameters_are_named_errors() {
    let build = |parameters| {
        let stage = stage(
            &textured(TINTED),
            ("fs_main", StageKind::Fragment),
            identity_table(),
            parameters,
        );
        ProgramLayout::build(&[&stage]).unwrap_err()
    };
    let mut loose = parameters(&[0], vec![texture(1, 2)], vec![sampler(2)]);
    loose
        .vectors
        .push(worldrunner_unity_serialized::shader_program::ValueParam {
            name_index: 0,
            index: 0,
            array_size: 1,
            kind: 0,
            dimension: 4,
        });
    assert_eq!(
        build(loose),
        LayoutError::UnsupportedParameter {
            kind: "loose vector".to_owned()
        }
    );
    let any = parameters(&[0], vec![texture(1, 1)], vec![sampler(2)]);
    assert_eq!(
        build(any),
        LayoutError::UnsupportedParameter {
            kind: "texture dimension 1".to_owned()
        }
    );
    let mut array = parameters(&[0], vec![texture(1, 2)], vec![sampler(2)]);
    array.constant_buffer_bindings[0].array_size = 4;
    assert_eq!(
        build(array),
        LayoutError::UnsupportedParameter {
            kind: "constant buffer array".to_owned()
        }
    );
}

#[test]
fn parameters_outside_the_binding_table_are_named_errors() {
    let resources = fragment_resources();
    let missing = |form| {
        let table = vec![same(0, 0), same(0, 2)];
        build(&[fragment_only(form, table, &resources)]).unwrap_err()
    };
    assert_eq!(
        missing(Form::Program),
        LayoutError::UnboundResource { unity_binding: 1 }
    );
    assert_eq!(
        missing(Form::Variant),
        LayoutError::UnboundSlot { set: 0, binding: 1 }
    );
}

#[test]
fn a_binding_that_is_in_several_descriptor_sets_is_ambiguous_without_a_set() {
    // Unity binding 0 is in sets 0 and 1. Editor-style parameters name no set.
    let table = vec![same(0, 0), same(1, 0), same(0, 1), same(0, 2)];
    let resources = fragment_resources();
    assert_eq!(
        build(&[fragment_only(Form::Program, table.clone(), &resources)]).unwrap_err(),
        LayoutError::AmbiguousBinding { unity_binding: 0 }
    );
    // A player variant names the set, which picks the entry.
    let layout = build(&[fragment_only(Form::Variant, table, &resources)]).unwrap();
    let slots: Vec<_> = layout.slots().iter().map(|s| (s.set, s.binding)).collect();
    assert_eq!(slots, [(0, 0), (0, 1), (0, 2)]);
}

#[test]
fn a_resource_without_a_parameter_is_a_named_error() {
    for form in FORMS {
        let no_texture = Resources::new(&[0], &[], &[2]);
        assert!(
            matches!(
                build(&[fragment_only(form, identity_table(), &no_texture)]).unwrap_err(),
                LayoutError::UndeclaredResource {
                    set: 0,
                    binding: 1,
                    ..
                }
            ),
            "{form:?}"
        );
    }
}

#[test]
fn a_parameter_of_another_kind_than_the_module_resource_is_a_named_error() {
    for form in FORMS {
        let cube = Resources::new(&[0], &[(1, Dim::Cube)], &[2]);
        assert!(
            matches!(
                build(&[fragment_only(form, identity_table(), &cube)]).unwrap_err(),
                LayoutError::Mismatch {
                    set: 0,
                    binding: 1,
                    declared: "a texture",
                    ..
                }
            ),
            "{form:?}"
        );
        // The texture's Unity binding is declared as a constant buffer.
        let wrong = Resources::new(&[0, 1], &[], &[2]);
        assert!(
            matches!(
                build(&[fragment_only(form, identity_table(), &wrong)]).unwrap_err(),
                LayoutError::Mismatch {
                    declared: "a constant buffer",
                    ..
                }
            ),
            "{form:?}"
        );
    }
    let mut multisampled = texture(1, 2);
    multisampled.multisampled = true;
    let ms = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        identity_table(),
        parameters(&[0], vec![multisampled], vec![sampler(2)]),
    );
    assert!(matches!(
        ProgramLayout::build(&[&ms]).unwrap_err(),
        LayoutError::Mismatch { binding: 1, .. }
    ));
}

#[test]
fn stages_that_declare_one_binding_differently_conflict() {
    for form in FORMS {
        let buffer = Resources::new(&[0], &[], &[]);
        let uniform_stage = stage_in(
            form,
            SHADER,
            ("vs_main", VERTEX),
            vec![same(0, 0)],
            &[(VERTEX, &buffer)],
        );
        let texture_resources = fragment_resources();
        let texture_stage = stage_in(
            form,
            &textured(TINTED),
            ("fs_main", FRAGMENT),
            identity_table(),
            &[(FRAGMENT, &texture_resources)],
        );
        // Same binding, same kind: merged.
        assert!(ProgramLayout::build(&[&uniform_stage, &texture_stage]).is_ok());
        let flipped_resources = Resources::new(&[], &[(0, Dim::D2)], &[1]);
        let flipped = stage_in(
            form,
            "@group(0) @binding(0) var t: texture_2d<f32>;
             @group(0) @binding(1) var s: sampler;
             @fragment fn fs_main() -> @location(0) vec4<f32> {
                 return textureSample(t, s, vec2<f32>(0.5, 0.5));
             }",
            ("fs_main", FRAGMENT),
            vec![same(0, 0), same(0, 1)],
            &[(FRAGMENT, &flipped_resources)],
        );
        assert!(
            matches!(
                ProgramLayout::build(&[&uniform_stage, &flipped]).unwrap_err(),
                LayoutError::Conflict {
                    set: 0,
                    binding: 0,
                    ..
                }
            ),
            "{form:?}"
        );
    }
}
