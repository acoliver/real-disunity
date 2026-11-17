//! CPU-only contracts for the early producer's ordered attachments and WGSL outputs.
use super::{TARGETS, fragment_outputs, module, writes_depth};
use crate::gbuffer_writes::{EarlyOutputs, FORWARD_OBJECTS, producer_depth};
use crate::renderer::depth::DepthMode;
use naga::ScalarKind;
use wgpu::{CompareFunction, StencilOperation, TextureFormat};

fn configurations() -> [EarlyOutputs; 4] {
    [
        EarlyOutputs {
            velocity: false,
            reflections: false,
        },
        EarlyOutputs {
            velocity: true,
            reflections: false,
        },
        EarlyOutputs {
            velocity: false,
            reflections: true,
        },
        EarlyOutputs {
            velocity: true,
            reflections: true,
        },
    ]
}

fn outputs(config: EarlyOutputs) -> Vec<(u32, ScalarKind, u32)> {
    let mut expected = TARGETS.to_vec();
    if config.velocity {
        expected.push((4, ScalarKind::Float, 2));
    }
    expected.push((config.normal_location(), ScalarKind::Float, 4));
    if config.reflections {
        expected.push((config.normal_location() + 1, ScalarKind::Float, 4));
    }
    expected
}

#[test]
fn ordered_targets_keep_canonical_metadata_within_seven_targets_and_46_bytes() {
    for (config, charge) in configurations().into_iter().zip([34, 38, 42, 46]) {
        let mut expected = crate::gbuffer::GBUFFER_FORMATS.to_vec();
        if config.velocity {
            expected.push(worldrunner_render_post::VELOCITY_FORMAT);
        }
        expected.push(TextureFormat::Rgba16Float);
        if config.reflections {
            expected.push(TextureFormat::Rgba16Float);
        }
        assert_eq!(config.formats(), expected);
        let charged = expected.iter().fold(0u32, |total, format| {
            total.next_multiple_of(format.target_component_alignment().unwrap())
                + format.target_pixel_byte_cost().unwrap()
        });
        assert_eq!(charged, charge);
        assert!(charged <= 46);
        assert!(expected.len() <= 7);
        let (object, terrain) = config.sources();
        for source in [&object, &terrain] {
            let targets =
                crate::renderer::base_source_targets_for_test(source, TextureFormat::Rgba16Float);
            let formats: Vec<_> = targets.into_iter().map(|t| t.unwrap().format).collect();
            assert_eq!(formats, expected);
            assert_eq!(EarlyOutputs::from_source(source), Some(config));
        }
    }
}

#[test]
fn every_early_object_entry_writes_metadata_and_keeps_relief_depth() {
    for config in configurations() {
        let (object, _) = config.sources();
        let parsed = module("early object producer", &object);
        for entry in FORWARD_OBJECTS {
            assert_eq!(fragment_outputs(&parsed, entry), outputs(config), "{entry}");
            assert_eq!(writes_depth(&parsed, entry), entry.starts_with("fs_relief"));
        }
    }
}

#[test]
fn every_early_terrain_variant_writes_the_same_ordered_targets() {
    for config in configurations() {
        let (_, terrain) = config.sources();
        for source in crate::terrain::program::source::test_sources(&terrain) {
            let parsed = module("early terrain producer", &source);
            assert_eq!(fragment_outputs(&parsed, "fs_terrain"), outputs(config));
            assert!(!writes_depth(&parsed, "fs_terrain"));
        }
    }
}

#[test]
fn observation_reads_equal_depth_and_early_render_writes_winners_and_stencil() {
    for config in configurations() {
        for format in [
            TextureFormat::Depth32Float,
            TextureFormat::Depth32FloatStencil8,
        ] {
            let mode = producer_depth(Some(config), format);
            let state = mode.state(true);
            assert_eq!(state.format, format);
            assert_eq!(state.depth_write_enabled, Some(true));
            assert_eq!(state.depth_compare, Some(CompareFunction::LessEqual));
            if format.has_stencil_aspect() {
                assert_eq!(state.stencil.front.pass_op, StencilOperation::Replace);
                assert_eq!(state.stencil.write_mask, 1);
            }
        }
    }
    let observation = producer_depth(None, TextureFormat::Depth32FloatStencil8);
    assert_eq!(observation, DepthMode::Reuse);
    assert_eq!(observation.state(true).depth_write_enabled, Some(false));
    assert_eq!(
        observation.state(true).depth_compare,
        Some(CompareFunction::Equal)
    );
    assert_eq!(observation.state(true).stencil.write_mask, 0);
    for velocity in [false, true] {
        let (object, terrain) = crate::gbuffer_writes::sources(velocity);
        assert_eq!(EarlyOutputs::from_source(&object), None);
        assert_eq!(EarlyOutputs::from_source(&terrain), None);
    }
}
