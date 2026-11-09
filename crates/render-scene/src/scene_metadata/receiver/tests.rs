use super::*;

#[test]
fn receiver_layout_is_lossless_and_checks_both_colour_limits() {
    for (layout, bytes, colors) in [
        (ReceiverLayout::Front, 24, 4),
        (ReceiverLayout::Opposing, 48, 6),
    ] {
        assert_eq!(layout.bytes(), bytes);
        let mut limits = wgpu::Limits {
            max_color_attachment_bytes_per_sample: bytes + 16,
            max_color_attachments: colors,
            ..Default::default()
        };
        assert_eq!(layout.validate(&limits, 2), Ok(()));
        limits.max_color_attachment_bytes_per_sample -= 1;
        assert!(layout.validate(&limits, 2).is_err());
        limits.max_color_attachment_bytes_per_sample += 1;
        limits.max_color_attachments -= 1;
        assert!(layout.validate(&limits, 2).is_err());
        assert_eq!(
            layout
                .allocations([65, 49])
                .validate(&wgpu::Limits::default())
                .unwrap(),
            u64::from(bytes) * 65 * 49
        );
    }
}

#[test]
fn receiver_reservations_charge_exact_layouts_only_in_the_paired_sample_domain() {
    use crate::compositor::{ReceiverGradientReservation as Reservation, RendererConfiguration};
    let mut settings = RendererConfiguration::legacy([65, 49], wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.ssao = Some(crate::ssr::neutral_ao());
    settings.receiver_gradients = Reservation::None;
    let limits = wgpu::Limits::default();
    let base = settings.allocations().validate(&limits).unwrap();
    for (reservation, bytes) in [(Reservation::Front, 24), (Reservation::Opposing, 48)] {
        settings.receiver_gradients = reservation;
        assert_eq!(
            settings.allocations().validate(&limits).unwrap() - base,
            bytes * 65 * 49
        );
    }
    for samples in [2, 4, 8] {
        settings.frame.samples = samples;
        settings.receiver_gradients = Reservation::None;
        let base = settings.allocations().validate(&limits).unwrap();
        settings.receiver_gradients = Reservation::Opposing;
        assert_eq!(settings.allocations().validate(&limits).unwrap(), base);
    }
    settings.ssao = None;
    settings.frame.samples = 1;
    settings.receiver_gradients = Reservation::None;
    let base = settings.allocations().validate(&limits).unwrap();
    settings.receiver_gradients = Reservation::Opposing;
    assert_eq!(settings.allocations().validate(&limits).unwrap(), base);
    assert!(!Reservation::None.admits(ReceiverLayout::Front));
    assert!(Reservation::Front.admits(ReceiverLayout::Front));
    assert!(!Reservation::Front.admits(ReceiverLayout::Opposing));
    assert!(Reservation::Opposing.admits(ReceiverLayout::Opposing));
}

fn validate(source: &str) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

#[test]
fn receiver_producers_and_consumers_validate_without_local_receiver_derivatives() {
    for layout in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
        for output in [
            crate::scene_metadata::MetadataOutput::Normal,
            crate::scene_metadata::MetadataOutput::Merged,
        ] {
            let sources = producer_sources(
                output,
                layout,
                0.012,
                crate::scene_metadata::receiver::ReceiverCapture::Normal,
            );
            for (source, input) in [(&sources.0, "input.world_position"), (&sources.1, "input")] {
                validate(source);
                assert_eq!(source_formats(source), layout.formats());
                assert!(source.contains(&format!(
                    "scene_capture_receiver({input}, projected.normal)"
                )));
            }
        }
        for reflection in [false, true] {
            for velocity in [false, true] {
                let sources = crate::shadows::receiver_sources(true, reflection, velocity);
                for (source, terrain) in [(&sources.0, false), (&sources.1, true)] {
                    let probe = probe_source(source);
                    validate(&probe);
                    validate(&consumer_source(&probe, layout, terrain));
                    let source = consumer_source(source, layout, terrain);
                    validate(&source);
                    assert!(!source.contains("dpdx(normal) * shadow.bias.y"));
                    assert!(!source.contains("dpdy(normal) * shadow.bias.y"));
                    assert!(source.contains("scene_receiver_pixel = vec2<i32>"));
                }
            }
        }
    }
}
