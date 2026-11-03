//! Bind group layouts from Unity parameters, binding tables and the module's own resources.

use wgpu::{BindingType, BufferBindingType, SamplerBindingType, ShaderStages};
use wgpu::{TextureSampleType, TextureViewDimension};
use worldrunner_render_program::layout::{LayoutError, ProgramLayout};
use worldrunner_render_program::module::{ResourceBinding, StageKind, StageProgram};

use crate::common::{parameters, same, sampler, stage, texture};
use crate::fixtures::textured;

const TEX2D: i8 = 2;
/// A fragment color that reads the material constants, like the vertex stage does.
const TINTED: &str = "texel * material.tint";
const SHADER: &str = "
@group(0) @binding(0) var<uniform> m: vec4<f32>;
@vertex fn vs_main() -> @builtin(position) vec4<f32> { return m; }";

fn identity_table() -> Vec<ResourceBinding> {
    (0..4).map(|b| same(0, b)).collect()
}

fn vertex() -> StageProgram {
    stage(
        &textured(TINTED),
        ("vs_main", StageKind::Vertex),
        identity_table(),
        parameters(&[0, 3], vec![], vec![]),
    )
}

fn fragment() -> StageProgram {
    stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        identity_table(),
        parameters(&[0], vec![texture(1, TEX2D)], vec![sampler(2)]),
    )
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
    let layout = ProgramLayout::build(&[&vertex(), &fragment()]).unwrap();
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
        ]
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

#[test]
fn a_split_combined_sampler_binds_the_sampler_the_table_names() {
    // Unity bound texture and sampler as one combined image sampler at 1; the split gave
    // the sampler binding 2 and moved the draw constants from Unity's 2 to 3.
    let table = vec![
        same(0, 0),
        ResourceBinding {
            set: 0,
            unity_binding: 1,
            binding: 1,
            sampler_binding: Some(2),
        },
        ResourceBinding {
            set: 0,
            unity_binding: 2,
            binding: 3,
            sampler_binding: None,
        },
    ];
    let split_vertex = stage(
        &textured(TINTED),
        ("vs_main", StageKind::Vertex),
        table.clone(),
        parameters(&[0, 2], vec![], vec![]),
    );
    let split_fragment = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        table,
        parameters(&[0], vec![texture(1, TEX2D)], vec![]),
    );
    let split = ProgramLayout::build(&[&split_vertex, &split_fragment]).unwrap();
    let plain = ProgramLayout::build(&[&vertex(), &fragment()]).unwrap();
    assert_eq!(split, plain);
}

#[test]
fn a_parameter_the_entry_point_does_not_use_is_reported_not_bound() {
    let extra = stage(
        &textured(TINTED),
        ("vs_main", StageKind::Vertex),
        identity_table(),
        parameters(&[0, 1, 3], vec![], vec![]),
    );
    let layout = ProgramLayout::build(&[&extra]).unwrap();
    assert_eq!(layout.slots().len(), 2);
    let unused: Vec<_> = layout.unused().iter().map(|u| (u.set, u.binding)).collect();
    assert_eq!(unused, [(0, 1)]);
}

#[test]
fn unsupported_parameters_are_named_errors() {
    let build = |parameters| {
        let stage = stage(
            &textured(TINTED),
            ("fs_main", StageKind::Fragment),
            identity_table(),
            parameters,
        );
        ProgramLayout::build(&[&stage]).unwrap_err()
    };
    let mut loose = parameters(&[0], vec![texture(1, TEX2D)], vec![sampler(2)]);
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
    let mut array = parameters(&[0], vec![texture(1, TEX2D)], vec![sampler(2)]);
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
    let missing = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        vec![same(0, 0), same(0, 2)],
        parameters(&[0], vec![texture(1, TEX2D)], vec![sampler(2)]),
    );
    assert_eq!(
        ProgramLayout::build(&[&missing]).unwrap_err(),
        LayoutError::UnboundResource { unity_binding: 1 }
    );
    let ambiguous = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        vec![same(0, 0), same(1, 0), same(0, 1), same(0, 2)],
        parameters(&[0], vec![texture(1, TEX2D)], vec![sampler(2)]),
    );
    assert_eq!(
        ProgramLayout::build(&[&ambiguous]).unwrap_err(),
        LayoutError::AmbiguousBinding { unity_binding: 0 }
    );
}

#[test]
fn a_resource_without_a_parameter_is_a_named_error() {
    let no_texture = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        identity_table(),
        parameters(&[0], vec![], vec![sampler(2)]),
    );
    assert!(matches!(
        ProgramLayout::build(&[&no_texture]).unwrap_err(),
        LayoutError::UndeclaredResource {
            set: 0,
            binding: 1,
            ..
        }
    ));
}

#[test]
fn a_parameter_of_another_kind_than_the_module_resource_is_a_named_error() {
    let cube = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        identity_table(),
        parameters(&[0], vec![texture(1, 4)], vec![sampler(2)]),
    );
    assert!(matches!(
        ProgramLayout::build(&[&cube]).unwrap_err(),
        LayoutError::Mismatch {
            set: 0,
            binding: 1,
            declared: "a texture",
            ..
        }
    ));
    let mut multisampled = texture(1, TEX2D);
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
    // The texture's Unity binding is declared as a constant buffer.
    let wrong = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        identity_table(),
        parameters(&[0, 1], vec![], vec![sampler(2)]),
    );
    assert!(matches!(
        ProgramLayout::build(&[&wrong]).unwrap_err(),
        LayoutError::Mismatch {
            declared: "a constant buffer",
            ..
        }
    ));
}

#[test]
fn stages_that_declare_one_binding_differently_conflict() {
    let uniform_stage = stage(
        SHADER,
        ("vs_main", StageKind::Vertex),
        vec![same(0, 0)],
        parameters(&[0], vec![], vec![]),
    );
    let texture_stage = stage(
        &textured(TINTED),
        ("fs_main", StageKind::Fragment),
        identity_table(),
        parameters(&[0], vec![texture(1, TEX2D)], vec![sampler(2)]),
    );
    // Same binding, same kind: merged.
    assert!(ProgramLayout::build(&[&uniform_stage, &texture_stage]).is_ok());
    let flipped = stage(
        "@group(0) @binding(0) var t: texture_2d<f32>;
         @group(0) @binding(1) var s: sampler;
         @fragment fn fs_main() -> @location(0) vec4<f32> {
             return textureSample(t, s, vec2<f32>(0.5, 0.5));
         }",
        ("fs_main", StageKind::Fragment),
        vec![same(0, 0), same(0, 1)],
        parameters(&[], vec![texture(0, TEX2D)], vec![sampler(1)]),
    );
    assert!(matches!(
        ProgramLayout::build(&[&uniform_stage, &flipped]).unwrap_err(),
        LayoutError::Conflict {
            set: 0,
            binding: 0,
            ..
        }
    ));
}
