//! Owned DXC fixtures must enter the ordinary stage/layout path without a texel binding.
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout, StorageAccess,
};
use worldrunner_render_program::layout::ProgramLayout;
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};

#[test]
fn cpu_unused_buffer_capabilities_enter_the_fragment_stage_path() {
    for name in ["unused-sampled-buffer", "unused-image-buffer", "no-buffer"] {
        let stage = stage(name, StageKind::Fragment);
        assert!(stage.reflected().is_empty());
        assert_eq!(
            stage.module().source,
            StageSource::SpirV(fixture(name).into())
        );
    }
}

#[test]
fn cpu_expanded_fetches_keep_all_four_components() {
    for base in ["float", "uint", "int"] {
        for suffix in ["", "2"] {
            let stage = stage(&format!("{base}{suffix}-expanded"), StageKind::Compute);
            let declared = stage.with_declarations(declarations(&stage));
            let layout = ProgramLayout::build(&[&declared]).unwrap();
            assert_eq!(layout.slot(0, 1).unwrap().min_size, 16);
        }
    }
}

#[test]
fn gpu_missing_components_match_vulkan_defaults() {
    use worldrunner_render_program::runtime::ProgramRuntime;
    let gpu = crate::common::Gpu::new();
    let mut runtime = ProgramRuntime::new(gpu.device, gpu.authority, crate::common::AMPLE);
    for (base, scalar) in [("float", "f32"), ("uint", "u32"), ("int", "i32")] {
        for components in [1, 2] {
            let suffix = if components == 1 { "" } else { "2" };
            let name = format!("{base}{suffix}-expanded");
            let element = if components == 1 {
                scalar.into()
            } else {
                format!("vec2<{scalar}>")
            };
            let typed = stage(&name, StageKind::Compute);
            let typed = typed.with_declarations(declarations(&typed));
            let padding = if components == 1 { "0, 0, 1" } else { "0, 1" };
            let text = format!(
                "
@group(0) @binding(0) var<storage, read> input: array<{element}>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<{scalar}>>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    output[id.x] = vec4<{scalar}>(input[id.x + 1u], {padding});
}}"
            );
            let reference = StageProgram::new(
                StageModule {
                    source: StageSource::Wgsl(text.into()),
                    entry_point: "main".into(),
                    kind: StageKind::Compute,
                },
                typed.declarations().clone(),
            )
            .unwrap();
            let input: Vec<u32> = (0..5 * components)
                .map(|i| {
                    if scalar == "f32" {
                        (f32::from(u16::try_from(i).unwrap()) - 7.0).to_bits()
                    } else {
                        (i32::try_from(i).unwrap() * 13 - 97).cast_unsigned()
                    }
                })
                .collect();
            let mut expected = vec![];
            for element in 1..5 {
                expected
                    .extend_from_slice(&input[element * components..(element + 1) * components]);
                expected.resize(expected.len() + 3 - components, 0);
                expected.push(if scalar == "f32" { 1.0f32.to_bits() } else { 1 });
            }
            let expected = crate::common::words_to_bytes(&expected);
            let typed_output = dispatch(&mut runtime, &typed, &input, expected.len());
            let reference_output = dispatch(&mut runtime, &reference, &input, expected.len());
            assert_eq!(typed_output, reference_output, "{name}");
            assert_eq!(typed_output, expected, "{name}");
        }
    }
}

#[test]
fn cpu_shared_image_types_keep_each_descriptor_address_and_source_words() {
    let stage = stage("multiple", StageKind::Compute);
    let resources: Vec<_> = stage
        .reflected()
        .iter()
        .map(|r| (r.set, r.binding, r.min_size))
        .collect();
    assert_eq!(resources, [(0, 1, 16), (2, 3, 4), (2, 5, 8), (2, 7, 4)]);
    assert_eq!(
        stage.module().source,
        StageSource::SpirV(fixture("multiple").into())
    );
    let declared = stage.with_declarations(declarations(&stage));
    ProgramLayout::build(&[&declared]).unwrap();
}

#[test]
fn cpu_legacy_and_modern_storage_classes_reflect_identically() {
    let baseline = stage("float", StageKind::Compute);
    for name in ["float-vk10", "float-vk12"] {
        let stage = stage(name, StageKind::Compute);
        let shape = |s: &StageProgram| {
            s.reflected()
                .iter()
                .map(|r| (r.set, r.binding, r.min_size, r.ty))
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&stage), shape(&baseline));
        let declared = stage.with_declarations(declarations(&stage));
        ProgramLayout::build(&[&declared]).unwrap();
    }
}

#[test]
fn cpu_refusals_reach_the_stage_api_with_the_descriptor_address() {
    use worldrunner_render_program::module::ModuleError;
    for (name, binding, expected) in [
        ("write", 1, "write"),
        ("query", 0, "size query"),
        ("r16f", 0, "format R16f needs conversion"),
        ("rgba8", 0, "format Rgba8 needs conversion"),
        ("unknown", 0, "format Unknown needs conversion"),
    ] {
        let result = StageProgram::new(
            StageModule {
                source: StageSource::SpirV(fixture(name).into()),
                entry_point: "main".into(),
                kind: StageKind::Compute,
            },
            StageDeclarations::default(),
        );
        match result.unwrap_err() {
            ModuleError::TexelBuffer {
                set,
                binding: actual,
                what,
            } => {
                assert_eq!((set, actual), (0, binding));
                assert!(what.contains(expected), "{name}: {what}");
            }
            other => panic!("{name}: {other}"),
        }
    }
}

fn fixture(name: &str) -> Vec<u32> {
    let path = format!(
        "{}/tests/fixtures/texel_buffer/{name}.spv",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

fn stage(name: &str, kind: StageKind) -> StageProgram {
    StageProgram::new(
        StageModule {
            source: StageSource::SpirV(fixture(name).into()),
            entry_point: "main".into(),
            kind,
        },
        StageDeclarations::default(),
    )
    .unwrap()
}

#[test]
fn typed_buffer_stages_reflect_read_only_storage_with_exact_element_stride() {
    for (name, stride, kind) in [
        ("palette", 16, StageKind::Vertex),
        ("float", 4, StageKind::Compute),
        ("float2", 8, StageKind::Compute),
        ("float4", 16, StageKind::Compute),
        ("uint", 4, StageKind::Compute),
        ("uint2", 8, StageKind::Compute),
        ("uint4", 16, StageKind::Compute),
        ("int", 4, StageKind::Compute),
        ("int2", 8, StageKind::Compute),
        ("int4", 16, StageKind::Compute),
    ] {
        let stage = stage(name, kind);
        let input = &stage.reflected()[0];
        assert_eq!(
            (input.set, input.binding, input.min_size),
            (0, 0, stride),
            "{name}"
        );
        assert!(
            matches!(
                input.ty,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    ..
                }
            ),
            "{name}: {input:?}"
        );
        let bindings = stage
            .reflected()
            .iter()
            .map(|resource| ResourceBinding {
                at: BindingAddress {
                    set: resource.set,
                    binding: resource.binding,
                },
                source: BindingSource::Unnamed,
                visibility: wgpu::ShaderStages::all(),
                kind: ResourceKind::of_binding_type(&resource.ty, resource.min_size).unwrap(),
            })
            .collect();
        let declared = stage.with_declarations(StageDeclarations {
            resources: ResourceLayout::new(bindings),
            constants: vec![],
        });
        let layout = ProgramLayout::build(&[&declared]).unwrap();
        let slot = layout.slot(0, 0).unwrap();
        assert_eq!(slot.min_size, stride);
        assert_eq!(
            ResourceKind::of_binding_type(&slot.ty, stride),
            Some(ResourceKind::Storage {
                access: StorageAccess::ReadOnly,
                min_size: stride,
            })
        );
    }
}

fn declarations(stage: &StageProgram) -> StageDeclarations {
    let bindings = stage
        .reflected()
        .iter()
        .map(|resource| ResourceBinding {
            at: BindingAddress {
                set: resource.set,
                binding: resource.binding,
            },
            source: BindingSource::Unnamed,
            visibility: wgpu::ShaderStages::all(),
            kind: ResourceKind::of_binding_type(&resource.ty, resource.min_size).unwrap(),
        })
        .collect();
    StageDeclarations {
        resources: ResourceLayout::new(bindings),
        constants: vec![],
    }
}

#[test]
fn gpu_typed_buffer_fetches_match_storage_buffers() {
    use worldrunner_render_program::runtime::ProgramRuntime;
    let gpu = crate::common::Gpu::new();
    let mut runtime = ProgramRuntime::new(gpu.device, gpu.authority, crate::common::AMPLE);
    for (base, scalar) in [("float", "f32"), ("uint", "u32"), ("int", "i32")] {
        for components in [1usize, 2, 4] {
            let suffix = if components == 1 {
                String::new()
            } else {
                components.to_string()
            };
            let name = format!("{base}{suffix}");
            let element = if components == 1 {
                scalar.into()
            } else {
                format!("vec{components}<{scalar}>")
            };
            let typed = stage(&name, StageKind::Compute);
            let typed = typed.with_declarations(declarations(&typed));
            let text = format!(
                "
@group(0) @binding(0) var<storage, read> input: array<{element}>;
@group(0) @binding(1) var<storage, read_write> output: array<{element}>;
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    output[id.x] = input[id.x + 1u];
}}"
            );
            let reference = StageProgram::new(
                StageModule {
                    source: StageSource::Wgsl(text.into()),
                    entry_point: "main".into(),
                    kind: StageKind::Compute,
                },
                typed.declarations().clone(),
            )
            .unwrap();
            let input: Vec<u32> = (0..components * 5)
                .map(|i| match scalar {
                    "f32" => (f32::from(u16::try_from(i).unwrap()) * 0.5 - 4.0).to_bits(),
                    "i32" => (i32::try_from(i).unwrap() * 13 - 101).cast_unsigned(),
                    _ => 0x8000_0000 + u32::try_from(i).unwrap() * 17,
                })
                .collect();
            let expected = crate::common::words_to_bytes(&input[components..]);
            let typed_output = dispatch(&mut runtime, &typed, &input, expected.len());
            let reference_output = dispatch(&mut runtime, &reference, &input, expected.len());
            assert_eq!(typed_output, reference_output, "{name}");
            assert_eq!(typed_output, expected, "{name}");
        }
    }
}

fn dispatch(
    runtime: &mut worldrunner_render_program::runtime::ProgramRuntime,
    stage: &StageProgram,
    input: &[u32],
    size: usize,
) -> Vec<u8> {
    use worldrunner_render_program::runtime::{BoundResource, DispatchRequest};
    let program = runtime.compute_program(stage).unwrap();
    let input = runtime
        .create_buffer("texel input", &crate::common::words_to_bytes(input))
        .unwrap();
    let output = runtime
        .create_buffer("texel output", &vec![0xcd; size])
        .unwrap();
    let authority = runtime.authority().clone();
    let device = runtime.device().clone();
    let mut frame = authority.begin(&device).unwrap();
    runtime
        .dispatch(
            &mut frame,
            &DispatchRequest {
                program: &program,
                bindings: &[
                    BoundResource::buffer(0, 0, input),
                    BoundResource::buffer(0, 1, output),
                ],
                groups: [4, 1, 1],
            },
        )
        .unwrap();
    let pending = runtime.read_buffer(&mut frame, output).unwrap();
    let index = frame.finish().submit().unwrap();
    let bytes = pending.resolve(index).unwrap();
    runtime.release_buffer(input).unwrap();
    runtime.release_buffer(output).unwrap();
    bytes
}
