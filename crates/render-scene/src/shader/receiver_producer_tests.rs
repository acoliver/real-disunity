//! Receiver capture and all transitive quad operations precede metadata rejection.
use super::*;
use crate::scene_metadata::{
    MetadataOutput,
    receiver::{ReceiverCapture, ReceiverLayout, function_source, producer_sources},
};

#[test]
fn zero_bias_receiver_producer_has_no_derivatives_after_early_rejection() {
    for output in [MetadataOutput::Normal, MetadataOutput::Merged] {
        for bias in [0.0, -0.0] {
            let (source, _) = producer_sources(
                output,
                ReceiverLayout::Front,
                bias,
                ReceiverCapture::Position,
            );
            assert!(check(&source, "reflection_metadata"));
            for root in [
                "fs_opaque",
                "fs_masked",
                "fs_relief_opaque",
                "fs_relief_masked",
            ] {
                assert!(check(&source, root), "{root}");
            }
            let body = function_source(&source, "reflection_metadata");
            let capture = "scene_capture_receiver(input.world_position, geometric);";
            let late = body
                .replace(capture, "")
                .replace("let surface =", &format!("{capture}\nlet surface ="));
            assert_ne!(body, late);
            assert!(!check(&source.replace(body, &late), "reflection_metadata"));
            for (name, expression) in [
                (
                    "scene_surface_channels_prepared",
                    "dpdx(input.world_position)",
                ),
                ("sample_normal_prepared", "dpdy(input.world_position)"),
                ("scene_project_decals_prepared", "dpdx(position)"),
            ] {
                let callee = function_source(&source, name);
                let changed = callee.replacen('{', &format!("{{ let mutation = {expression};"), 1);
                assert!(
                    !check(&source.replace(callee, &changed), "reflection_metadata"),
                    "{name}"
                );
            }
        }
    }
}
