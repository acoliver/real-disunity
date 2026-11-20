use super::*;
use rspirv::dr::{Module, Operand};
use rspirv::spirv::{Capability, ImageFormat};

fn words(name: &str) -> Vec<u32> {
    let path = format!(
        "{}/tests/fixtures/texel_buffer/{name}.spv",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

fn changed(edit: impl FnOnce(&mut Module)) -> Vec<u32> {
    let mut module = rspirv::dr::load_words(words("float")).unwrap();
    edit(&mut module);
    module.assemble()
}

fn refuse(words: &[u32], expected: &str) {
    refuse_at(words, expected, (0, 0));
}

fn refuse_at(words: &[u32], expected: &str, address: (u32, u32)) {
    match lower(words).unwrap_err() {
        ModuleError::TexelBuffer { set, binding, what } => {
            assert_eq!((set, binding), address);
            assert!(
                what.contains(expected),
                "{what:?} does not contain {expected:?}"
            );
        }
        other => panic!("expected texel-buffer refusal, got {other:?}"),
    }
}

#[test]
fn a_shared_image_types_refusal_names_the_resource_that_was_used() {
    let mut module = rspirv::dr::load_words(words("multiple")).unwrap();
    let variable = module
        .annotations
        .iter()
        .find_map(|i| match i.operands.as_slice() {
            [
                Operand::IdRef(var),
                Operand::Decoration(rspirv::spirv::Decoration::Binding),
                Operand::LiteralBit32(7),
            ] => Some(*var),
            _ => None,
        })
        .unwrap();
    let loaded = module
        .all_inst_iter()
        .find(|i| i.class.opcode == Op::Load && i.operands == [Operand::IdRef(variable)])
        .unwrap()
        .result_id
        .unwrap();
    for i in module.all_inst_iter_mut() {
        if i.class.opcode == Op::ImageFetch && i.operands[0] == Operand::IdRef(loaded) {
            *i = instruction(
                Op::ImageQuerySize,
                i.result_type,
                i.result_id,
                vec![Operand::IdRef(loaded)],
            );
        }
    }
    refuse_at(&module.assemble(), "size query", (2, 7));
}

#[test]
fn unused_capability_scan_leaves_malformed_streams_for_naga() {
    for tail in [vec![0], vec![3 << 16]] {
        let mut words = words("unused-sampled-buffer");
        words.extend(tail);
        let lowered = lower(&words).unwrap();
        assert!(matches!(lowered, Cow::Borrowed(_)));
        assert!(std::ptr::eq(lowered.as_ptr(), words.as_ptr()));
    }
}

#[test]
fn no_buffer_image_or_capability_borrows_identical_words() {
    let words = words("no-buffer");
    let lowered = lower(&words).unwrap();
    assert!(matches!(lowered, Cow::Borrowed(_)));
    assert!(std::ptr::eq(lowered.as_ptr(), words.as_ptr()));
    assert_eq!(&*lowered, words);
}

fn check_unused_capabilities(words: &[u32], removed: usize) {
    assert!(!has_buffer_image(words));
    let original = rspirv::dr::load_words(words).unwrap();
    let lowered = lower(words).unwrap();
    assert!(matches!(lowered, Cow::Owned(_)));
    let module = rspirv::dr::load_words(&*lowered).unwrap();
    assert_eq!(
        module.all_inst_iter().count() + removed,
        original.all_inst_iter().count()
    );
    let mut expected = words[..5].to_vec();
    for i in original.all_inst_iter() {
        if i.class.opcode != Op::Capability
            || !matches!(
                i.operands.as_slice(),
                [Operand::Capability(
                    Capability::SampledBuffer | Capability::ImageBuffer
                )]
            )
        {
            expected.extend(i.assemble());
        }
    }
    assert_eq!(&*lowered, expected);
    assert_eq!(lowered.len() + 2 * removed, words.len());
    let bytes: Vec<_> = lowered.iter().flat_map(|word| word.to_le_bytes()).collect();
    let module =
        naga::front::spv::parse_u8_slice(&bytes, &naga::front::spv::Options::default()).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    assert!(matches!(lower(&lowered).unwrap(), Cow::Borrowed(_)));
}

#[test]
fn unused_buffer_capabilities_are_the_only_removed_instructions() {
    check_unused_capabilities(&words("unused-sampled-buffer"), 1);
    // DXC emits both capabilities for an unused RWBuffer global.
    check_unused_capabilities(&words("unused-image-buffer"), 2);
    let mut image_only = rspirv::dr::load_words(words("unused-image-buffer")).unwrap();
    image_only
        .capabilities
        .retain(|i| i.operands != [Operand::Capability(Capability::SampledBuffer)]);
    check_unused_capabilities(&image_only.assemble(), 1);
}

#[test]
fn writes_and_size_queries_are_named() {
    refuse_at(&words("write"), "write", (0, 1));
    refuse(&words("query"), "size query");
}

#[test]
fn all_non_exact_formats_are_refused_by_name() {
    for format in 0..=41 {
        let format = ImageFormat::from_u32(format).unwrap();
        if matches!(
            format,
            ImageFormat::Rgba32f
                | ImageFormat::Rg32f
                | ImageFormat::R32f
                | ImageFormat::Rgba32i
                | ImageFormat::Rg32i
                | ImageFormat::R32i
                | ImageFormat::Rgba32ui
                | ImageFormat::Rg32ui
                | ImageFormat::R32ui
        ) {
            continue;
        }
        let words = changed(|m| {
            m.types_global_values
                .iter_mut()
                .find(|i| i.class.opcode == Op::TypeImage)
                .unwrap()
                .operands[6] = Operand::ImageFormat(format);
        });
        refuse(&words, &format!("format {format:?} needs conversion"));
    }
}

#[test]
fn wide_or_mismatched_sampled_types_are_refused() {
    let words = changed(|m| {
        m.types_global_values
            .iter_mut()
            .find(|i| i.class.opcode == Op::TypeFloat)
            .unwrap()
            .operands[0] = Operand::LiteralBit32(64);
    });
    refuse(&words, "non-exact sampled type");
}

#[test]
fn arrayed_images_and_descriptor_arrays_are_named() {
    let words = changed(|m| {
        m.types_global_values
            .iter_mut()
            .find(|i| i.class.opcode == Op::TypeImage)
            .unwrap()
            .operands[3] = Operand::LiteralBit32(1);
    });
    refuse(&words, "arrays of buffer images");
    let words = changed(|m| {
        let image = m
            .types_global_values
            .iter()
            .find(|i| i.class.opcode == Op::TypeImage)
            .unwrap()
            .result_id
            .unwrap();
        let array = m.header.as_mut().unwrap().bound;
        m.header.as_mut().unwrap().bound += 1;
        for i in &mut m.types_global_values {
            if i.class.opcode == Op::TypePointer && i.operands[1] == Operand::IdRef(image) {
                i.operands[1] = Operand::IdRef(array);
            }
        }
        m.types_global_values.push(instruction(
            Op::TypeRuntimeArray,
            None,
            Some(array),
            vec![Operand::IdRef(image)],
        ));
    });
    refuse(&words, "arrays of buffer images");
}

#[test]
fn sparse_fetch_copy_object_and_sampling_are_named() {
    for (op, name) in [
        (Op::ImageSparseFetch, "sparse fetch"),
        (Op::CopyObject, "OpCopyObject"),
        (Op::ImageSampleImplicitLod, "non-fetch use"),
    ] {
        let words = changed(|m| {
            let fetch = m.functions[0].blocks[0]
                .instructions
                .iter_mut()
                .find(|i| i.class.opcode == Op::ImageFetch)
                .unwrap();
            let operands = if op == Op::CopyObject {
                fetch.operands[..1].to_vec()
            } else {
                fetch.operands.clone()
            };
            *fetch = instruction(op, fetch.result_type, fetch.result_id, operands);
        });
        refuse(&words, name);
    }
}

#[test]
fn image_function_parameters_are_named() {
    let words = changed(|m| {
        let image = m
            .types_global_values
            .iter()
            .find(|i| i.class.opcode == Op::TypeImage)
            .unwrap()
            .result_id
            .unwrap();
        m.types_global_values
            .iter_mut()
            .find(|i| i.class.opcode == Op::TypeFunction)
            .unwrap()
            .operands
            .push(Operand::IdRef(image));
    });
    refuse(&words, "function parameter");
}

#[test]
fn fetch_operands_are_not_silently_dropped() {
    let words = changed(|m| {
        let fetch = m.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find(|i| i.class.opcode == Op::ImageFetch)
            .unwrap();
        fetch.operands[2] = Operand::ImageOperands(rspirv::spirv::ImageOperands::LOD);
        fetch.operands.push(fetch.operands[1].clone());
    });
    refuse(&words, "image fetch operands");
}

#[test]
fn exact_formats_have_element_strides_and_vulkan_missing_components() {
    for (name, components, one) in [
        ("float", 1, 1.0f32.to_bits()),
        ("float2", 2, 1.0f32.to_bits()),
        ("float4", 4, 1.0f32.to_bits()),
        ("uint", 1, 1),
        ("uint2", 2, 1),
        ("uint4", 4, 1),
        ("int", 1, 1),
        ("int2", 2, 1),
        ("int4", 4, 1),
    ] {
        let words = words(name);
        let lowered = lower(&words).unwrap();
        let module = rspirv::dr::load_words(&*lowered).unwrap();
        assert!(!has_buffer_image(&lowered));
        assert!(
            !module
                .capabilities
                .iter()
                .any(|i| i.operands == [Operand::Capability(Capability::SampledBuffer)])
        );
        assert!(module.annotations.iter().any(|i| i.operands.get(1..)
            == Some(&[
                Operand::Decoration(rspirv::spirv::Decoration::ArrayStride),
                Operand::LiteralBit32(components * 4)
            ])));
        if components < 4 {
            let construct = module.functions[0].blocks[0]
                .instructions
                .iter()
                .find(|i| i.class.opcode == Op::CompositeConstruct)
                .unwrap();
            assert_eq!(construct.operands.len(), 4);
            for component in components..4 {
                let constant = id(&construct.operands[component as usize]).unwrap();
                let literal = if component == 3 { one } else { 0 };
                assert!(
                    module
                        .types_global_values
                        .iter()
                        .any(|i| i.result_id == Some(constant)
                            && i.operands == [Operand::LiteralBit32(literal)])
                );
            }
        }
        assert_eq!(&*lower(&lowered).unwrap(), &*lowered);
    }
}

#[test]
fn legacy_versions_get_the_storage_buffer_extension() {
    let words = changed(|m| m.header.as_mut().unwrap().version = 0x0001_0000);
    let lowered = lower(&words).unwrap();
    let module = rspirv::dr::load_words(&*lowered).unwrap();
    assert!(module.extensions.iter().any(|i| i.operands
        == [Operand::LiteralString(
            "SPV_KHR_storage_buffer_storage_class".into()
        )]));
    assert_eq!(module.header.unwrap().version, 0x0001_0000);
}

#[test]
fn literals_equal_to_resource_ids_are_not_uses() {
    let words = changed(|m| {
        let load = m.functions[0].blocks[0]
            .instructions
            .iter()
            .find(|i| {
                i.class.opcode == Op::Load
                    && i.result_type.is_some_and(|ty| {
                        m.types_global_values
                            .iter()
                            .any(|t| t.result_id == Some(ty) && t.class.opcode == Op::TypeImage)
                    })
            })
            .unwrap()
            .result_id
            .unwrap();
        m.types_global_values
            .iter_mut()
            .find(|i| i.class.opcode == Op::Constant)
            .unwrap()
            .operands[0] = Operand::LiteralBit32(load);
    });
    lower(&words).unwrap();
}

#[test]
fn truncated_buffer_modules_return_parse_errors() {
    let mut words = words("float");
    words.pop();
    assert!(matches!(lower(&words), Err(ModuleError::Parse(_))));
}
