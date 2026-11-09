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
                assert!(capture.contains("dpdx(position) + vec3<f32>(0.0)"));
                assert!(capture.contains("dpdy(position) + vec3<f32>(0.0)"));
                assert_eq!(source_formats(&source), ReceiverLayout::Front.formats());
            }
        }
    }
}

#[test]
#[should_panic(expected = "evaluation == ReceiverCapture::Normal")]
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
