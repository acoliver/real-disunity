//! Registering programs: what a program must be for its handle to exist, and the SPIR-V path.

use std::sync::Arc;

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use wgpu::ShaderStages;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::layout::LayoutError;
use worldrunner_render_program::module::{ModuleError, StageDeclarations, StageKind};
use worldrunner_render_program::registry::{ProgramDesc, StageDesc, VariantDesc};
use worldrunner_render_program::state::{RenderState, StencilState};
use worldrunner_render_program::variant::VariantError;

use crate::common::{
    COLOR, Gpu, Rig, WGSL, assert_pixel, colored, draw_of, features, no_cull, rectangle,
    tint_declarations, variant,
};

fn desc(variants: Vec<VariantDesc>) -> ProgramDesc {
    ProgramDesc {
        label: "program".to_owned(),
        state: RenderState::opaque_depth(),
        variants,
    }
}

#[test]
fn a_program_without_variants_or_with_a_state_a_draw_refuses_is_not_registered() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    assert_eq!(
        runtime.register_program(desc(Vec::new())).unwrap_err(),
        ProgramError::NoVariants {
            program: "program".to_owned()
        }
    );
    let mut stencil = desc(vec![variant(&[], "vs", "fs")]);
    stencil.state.stencil = Some(StencilState {
        ops: wgpu::StencilState::default(),
        reference: 1,
    });
    assert_eq!(
        runtime.register_program(stencil).unwrap_err(),
        ProgramError::UnsupportedState { what: "stencil" }
    );
}

#[test]
fn stages_that_do_not_parse_or_match_their_declarations_are_named_errors() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let broken = StageDesc::wgsl(
        "fn broken(",
        "vs",
        StageKind::Vertex,
        StageDeclarations::default(),
    );
    let mut parse = variant(&[], "vs", "fs");
    parse.vertex = broken;
    assert!(matches!(
        runtime.register_program(desc(vec![parse])).unwrap_err(),
        ProgramError::Module(ModuleError::Parse(_))
    ));

    let missing = variant(&[], "vs", "nowhere");
    assert_eq!(
        runtime.register_program(desc(vec![missing])).unwrap_err(),
        ProgramError::Module(ModuleError::EntryPointMissing {
            name: "nowhere".to_owned(),
            kind: StageKind::Fragment
        })
    );

    let mut undeclared = variant(&[], "vs", "fs");
    undeclared.fragment.declarations = StageDeclarations::default();
    assert!(matches!(
        runtime
            .register_program(desc(vec![undeclared]))
            .unwrap_err(),
        ProgramError::Layout(LayoutError::UndeclaredResource {
            set: 0,
            binding: 0,
            ..
        })
    ));
}

#[test]
fn two_variants_with_one_feature_set_are_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let twice = desc(vec![
        variant(&["FOG"], "vs", "fs"),
        variant(&["FOG"], "vs", "fs_double"),
    ]);
    assert_eq!(
        runtime.register_program(twice).unwrap_err(),
        ProgramError::Variant(VariantError::DuplicateVariant {
            features: features(&["FOG"]).into_iter().collect()
        })
    );
}

#[test]
fn constants_that_differ_between_stages_or_variants_are_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();

    let mut stages = variant(&[], "vs", "fs");
    stages.vertex.declarations = tint_declarations(ShaderStages::VERTEX);
    stages.vertex.declarations.constants[0].layout.byte_size = 32;
    let error = runtime.register_program(desc(vec![stages])).unwrap_err();
    assert!(
        matches!(&error, ProgramError::Layout(LayoutError::Declaration { reason })
            if reason.contains("set 0 binding 0")),
        "{error:?}"
    );

    let mut scalar = variant(&["SCALAR"], "vs", "fs");
    scalar.fragment.declarations.constants[0].layout.members[0].cols = 1;
    let error = runtime
        .register_program(desc(vec![variant(&[], "vs", "fs"), scalar]))
        .unwrap_err();
    assert_eq!(
        error,
        ProgramError::PropertyConflict {
            program: "program".to_owned(),
            property: "color".to_owned()
        }
    );
}

fn spirv_words() -> Arc<[u32]> {
    let module = naga::front::wgsl::parse_str(WGSL).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    let options = spv::Options {
        flags: spv::WriterFlags::empty(),
        ..spv::Options::default()
    };
    Arc::from(spv::write_vec(&module, &info, &options, None).unwrap())
}

/// SPIR-V stages register and draw through the same calls as WGSL ones, and keep their own clip
/// convention: positive clip Y lands in the bottom rows, where WGSL puts it in the top rows.
#[test]
fn spirv_stages_register_like_wgsl_ones_and_keep_the_spirv_clip_convention() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let words = spirv_words();
    let vertex = StageDesc::spirv(
        words.clone(),
        "vs",
        StageKind::Vertex,
        StageDeclarations::default(),
    );
    let fragment = StageDesc::spirv(
        words,
        "fs",
        StageKind::Fragment,
        tint_declarations(ShaderStages::FRAGMENT),
    );
    let program = rig
        .runtime
        .register_program(ProgramDesc::single("spirv", no_cull(), vertex, fragment))
        .unwrap();
    rig.material = rig
        .runtime
        .create_material(program, colored(COLOR))
        .unwrap();
    let top_half = rectangle(&mut rig.runtime, [-1.0, 0.0, 1.0, 1.0], 0.5);
    let image = rig.render(&draw_of(rig.material, top_half, 6)).unwrap();
    for x in 0..4 {
        assert_pixel(&image, (x, 0), [0.0; 4]);
        assert_pixel(&image, (x, 1), [0.0; 4]);
        assert_pixel(&image, (x, 2), COLOR);
        assert_pixel(&image, (x, 3), COLOR);
    }
}
