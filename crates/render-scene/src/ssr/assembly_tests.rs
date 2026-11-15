//! Every trace, reduction and resolve variant assembles: each anchored rewrite of each variant
//! finds its anchor the expected number of times.

use super::*;
use worldrunner_render_plan::{
    SceneDownsampleRounding, SceneSsaoReceiver, SceneSsrGridDomain, SceneSsrHitFacing,
    SceneSsrPyramid, SceneSsrTraversal,
};

fn settings() -> impl Iterator<Item = SceneSsr> {
    let facings = [
        SceneSsrHitFacing::ShadingNormal,
        SceneSsrHitFacing::Any,
        SceneSsrHitFacing::AnyForWaterReceivers,
    ];
    let traversals = [SceneSsrTraversal::Conservative, SceneSsrTraversal::Grid];
    let domains = [
        (SceneSsrGridDomain::FullResolution, 1_000_000),
        (SceneSsrGridDomain::ReflectionOutput, 500_000),
    ];
    let pyramids = [SceneSsrPyramid::SixLevels, SceneSsrPyramid::Full];
    let roundings = [
        SceneDownsampleRounding::Ceil,
        SceneDownsampleRounding::Floor,
    ];
    facings.into_iter().flat_map(move |hit_facing| {
        traversals.into_iter().flat_map(move |traversal| {
            domains
                .into_iter()
                .flat_map(move |(grid_domain, resolution_scale_micro)| {
                    pyramids.into_iter().flat_map(move |radiance_pyramid| {
                        roundings
                            .into_iter()
                            .map(move |radiance_rounding| SceneSsr {
                                hit_facing,
                                traversal,
                                grid_domain,
                                resolution_scale_micro,
                                radiance_pyramid,
                                radiance_rounding,
                                ..Default::default()
                            })
                    })
                })
        })
    })
}

#[test]
fn every_trace_and_reduction_variant_assembles() {
    for settings in settings() {
        for samples in [1, 4] {
            for water in [false, true] {
                for size in [[65, 65], [640, 360]] {
                    let trace = grid::trace_source(samples, water, &settings, size);
                    assert!(trace.contains("@fragment fn fs_screen"));
                }
            }
            for dual_output in [false, true] {
                let reduction = pyramid::reduction_source(samples, &settings, dual_output);
                assert!(reduction.contains("@fragment fn fs_screen"));
            }
        }
    }
}

#[test]
fn every_resolve_variant_assembles() {
    let inputs = [
        (SceneSsaoReceiver::Nearest, wgpu::TextureFormat::R32Float),
        (SceneSsaoReceiver::Nearest, wgpu::TextureFormat::Rg16Float),
        (
            SceneSsaoReceiver::DepthAwareGather,
            wgpu::TextureFormat::Rg32Float,
        ),
    ];
    for size in [[65, 65], [640, 360], [1920, 1080]] {
        let extents = SceneSsr {
            radiance_pyramid: SceneSsrPyramid::Full,
            ..Default::default()
        }
        .radiance_extents(size);
        for samples in [1, 4] {
            for water in [false, true] {
                for ao in [false, true] {
                    for ao_input in inputs {
                        for banked in [false, true] {
                            let source = gpu::configured_resolve_source_variant(
                                extents.len(),
                                samples,
                                water,
                                ao,
                                ao_input,
                                banked,
                            );
                            let shared = pyramid::resolve_source_variant(
                                source,
                                &extents,
                                samples,
                                banked,
                                gpu::trailing_bindings(water, ao),
                            );
                            assert!(shared.contains("fn reflected("));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn water_receiver_hit_facing_assembles_over_the_water_trace() {
    for samples in [1, 4] {
        for rounding in [
            SceneDownsampleRounding::Ceil,
            SceneDownsampleRounding::Floor,
        ] {
            let trace = gpu::configured_trace_source(samples, true, rounding);
            let facing = water::hit_facing_source(&trace);
            assert!(facing.contains("fn water_receiver_policy("));
            assert!(!facing.contains(" || dot(hit_normal.xyz, direction) >= 0.0"));
        }
    }
}
