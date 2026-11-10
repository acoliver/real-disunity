//! Zero-bias receivers capture quad operands before coverage rejection.
use super::*;

#[test]
fn zero_bias_producer_rejects_before_evaluating_normal_surface_and_decals() {
    for output in [
        super::super::MetadataOutput::Normal,
        super::super::MetadataOutput::Merged,
    ] {
        let (source, _) = producer_sources(
            output,
            ReceiverLayout::Front,
            0.0,
            ReceiverCapture::Position,
        );
        let body = function_source(&source, "reflection_metadata");
        let rejection = body.find("if !scene_geometry_covered(input)").unwrap();
        assert!(body.find("scene_capture_receiver(").unwrap() < rejection);
        for name in [
            "scene_surface_channels_prepared(",
            "sample_normal_prepared(",
            "scene_contact(",
            "scene_project_decals_prepared(",
        ] {
            assert!(body.find(name).unwrap() > rejection, "{name}");
        }
    }
}

#[test]
fn receiver_capture_specializes_only_exact_zero_front_without_transmission() {
    for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
        for transmission in [false, true] {
            for bias in [0.0, -0.0, f32::from_bits(1), -f32::from_bits(1), 0.012] {
                let expected = if layout == ReceiverLayout::Front && bias == 0.0 && !transmission {
                    ReceiverCapture::Position
                } else {
                    ReceiverCapture::Normal
                };
                assert_eq!(
                    ReceiverCapture::select(layout, bias, transmission),
                    expected
                );
            }
        }
    }
}

#[test]
fn position_capture_sources_validate_for_both_signed_zeros_and_outputs() {
    for output in [
        super::super::MetadataOutput::Normal,
        super::super::MetadataOutput::Merged,
    ] {
        for bias in [0.0, -0.0] {
            let (object, terrain) = producer_sources(
                output,
                ReceiverLayout::Front,
                bias,
                ReceiverCapture::Position,
            );
            for source in [object, terrain] {
                let module = naga::front::wgsl::parse_str(&source).unwrap();
                naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                )
                .validate(&module)
                .unwrap();
                let capture = function_source(&source, "scene_capture_receiver");
                assert!(!capture.contains("dpdx(normal)"));
                assert!(!capture.contains("dpdy(normal)"));
                assert!(capture.contains("let dx_0 = dpdx(position);"));
                assert!(capture.contains("let dy_0 = dpdy(position);"));
                assert_eq!(source_formats(&source), ReceiverLayout::Front.formats());
            }
        }
    }
}

#[test]
fn position_capture_survives_every_terrain_runtime_surface() {
    for output in [
        super::super::MetadataOutput::Normal,
        super::super::MetadataOutput::Merged,
    ] {
        for bias in [0.0, -0.0] {
            let (_, terrain) = producer_sources(
                output,
                ReceiverLayout::Front,
                bias,
                ReceiverCapture::Position,
            );
            for source in crate::terrain::program::source::test_sources(&terrain) {
                let body = function_source(&source, "fs_terrain");
                assert_eq!(body.matches("scene_capture_receiver(").count(), 1);
                assert!(body.contains("receiver_0_a, receiver_0_b"));
                assert_eq!(source_formats(&source), ReceiverLayout::Front.formats());
                let capture = function_source(&source, "scene_capture_receiver");
                assert!(capture.contains("let dx_0 = dpdx(position);"));
                assert!(capture.contains("let dy_0 = dpdy(position);"));
                let module = naga::front::wgsl::parse_str(&source).unwrap();
                naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                )
                .validate(&module)
                .unwrap();
            }
        }
    }
}

#[test]
#[should_panic(expected = "evaluation != ReceiverCapture::Position")]
fn position_capture_rejects_nonzero_bias() {
    producer_sources(
        super::super::MetadataOutput::Merged,
        ReceiverLayout::Front,
        f32::from_bits(1),
        ReceiverCapture::Position,
    );
}

#[test]
fn zero_bias_ieee_arithmetic_needs_fragment_backend_controls() {
    assert!((f32::NAN * 0.0).is_nan());
    assert!((f32::INFINITY * -0.0).is_nan());
    assert_eq!((-0.0_f32 + 0.0).to_bits(), 0);
    assert_eq!((-0.0_f32 + -0.0).to_bits(), 0x8000_0000);
}

#[test]
fn normal_capture_also_uses_position_derivatives_at_either_zero_bias() {
    for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
        for bias in [0.0, -0.0] {
            let (object, terrain) = producer_sources(
                super::super::MetadataOutput::Merged,
                layout,
                bias,
                ReceiverCapture::Normal,
            );
            for source in [object, terrain] {
                let capture = function_source(&source, "scene_capture_receiver");
                assert!(!capture.contains("dpdx(normal)"));
                assert!(!capture.contains("dpdy(normal)"));
                for side in 0..layout.sides() {
                    assert!(capture.contains(&format!("let dx_{side} = dpdx(position);")));
                    assert!(capture.contains(&format!("let dy_{side} = dpdy(position);")));
                }
            }
        }
    }
}

#[test]
fn local_receivers_select_exact_position_and_derivatives_in_uniform_flow() {
    let source = include_str!("../../shader/shadows.wgsl");
    assert!(
        source
            .contains("select(position + normal * shadow.bias.y, position, shadow.bias.y == 0.0)")
    );
    for name in ["cascaded_sunlight_visibility", "scene_prepare_card_shadow"] {
        let body = function_source(source, name);
        for axis in ["x", "y"] {
            assert!(body.contains(&format!(
                "bitcast<vec3<f32>>(select(bitcast<vec3<u32>>(dpd{axis}(position) + dpd{axis}(normal) * shadow.bias.y), bitcast<vec3<u32>>(dpd{axis}(position)), shadow.bias.y == 0.0))"
            )));
        }
    }
    assert_eq!(source.matches("vec4<f32>(select(position + normal * shadow.bias.y, position, shadow.bias.y == 0.0), 1.0)").count(), 5);
}
