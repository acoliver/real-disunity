use super::*;

#[path = "footprint_fixture.rs"]
mod fixture;
#[path = "footprint_mip_tests.rs"]
mod mip_tests;

#[derive(Clone, Copy, Debug)]
enum Edge {
    None,
    Ownership,
    Depth,
    Normal,
}

#[derive(Clone, Copy, Debug)]
struct Case {
    size: [u32; 2],
    rounding: worldrunner_render_plan::SceneDownsampleRounding,
    scale: u32,
    roughness: f32,
    samples: u32,
    edge: Edge,
    ao: bool,
    full: bool,
    water: bool,
}

const SIZE: [u32; 2] = [17, 13];

fn policy(case: Case) -> SceneSsr {
    SceneSsr {
        resolution_scale_micro: case.scale,
        radiance_rounding: case.rounding,
        roughness_cutoff_disabled: true,
        application_lod: worldrunner_render_plan::SceneSsrLodMapping::RoughnessDownsample,
        composition: worldrunner_render_plan::SceneSsrComposition::ReplaceEnvironment,
        radiance_pyramid: if case.full {
            worldrunner_render_plan::SceneSsrPyramid::Full
        } else {
            worldrunner_render_plan::SceneSsrPyramid::SixLevels
        },
        depth_relative_ao: case.ao,
        ..settings()
    }
}

// Integrate the separable triangular reconstruction basis over its discrete
// trace samples. This oracle enumerates the entire logical image, independently
// of the shader's bounded gather and physical lane packing.
fn reference(case: Case, lane: u32) -> [f32; 4] {
    let sizes = policy(case).radiance_extents(case.size);
    let [width, height] = sizes[0];
    let lod = (case.roughness * gpu::scalar(u32::try_from(sizes.len() - 1).unwrap())
        - if case.scale == 500_000 { 1.0 } else { 0.0 })
    .max(0.0);
    let radius = (2.0 * lod.exp2()).clamp(2.0, 4.0);
    let mut denominator = 0.0;
    let mut numerator = 0.0;
    for y in 0..height {
        for x in 0..width {
            let incompatible = !matches!(case.edge, Edge::None) && x > width / 2;
            if incompatible {
                continue;
            }
            let wx = (radius - gpu::scalar(x.abs_diff(width / 2))).max(0.0);
            let wy = (radius - gpu::scalar(y.abs_diff(height / 2))).max(0.0);
            denominator += wx * wy;
            if x == width / 2 && y == height / 2 {
                numerator = wx * wy;
            }
        }
    }
    let coverage = numerator / denominator / gpu::scalar(lane + 1);
    [0.25 * coverage, 0.5 * coverage, 0.75 * coverage, coverage]
}

fn check_case(context: &GpuContext, case: Case, failures: &mut Vec<String>) {
    if reflection_lod(case) > 1.0 {
        fixture::check_mip_kernel(context, case);
        return;
    }
    let (pairs, deltas) = fixture::capture(context, case);
    for lane in 0..case.samples {
        let expected = reference(case, lane);
        let index = usize::try_from((SIZE[1] / 2 * SIZE[0] + SIZE[0] / 2) * 4).unwrap();
        let pair = &pairs[usize::try_from(lane).unwrap()][index..index + 4];
        let delta = &deltas[usize::try_from(lane).unwrap()][index..index + 4];
        for channel in 0..4 {
            if (pair[channel] - expected[channel]).abs() > 0.0005 {
                failures.push(format!(
                    "pair {case:?}, lane {lane}, channel {channel}: {} != {}",
                    pair[channel], expected[channel]
                ));
            }
        }
        for channel in 0..3 {
            // Water F0=1 and LUT=(0.5,0): factor=0.5; published environment=2.
            let replaced = expected[channel] * 0.5 - 2.0 * expected[3];
            if (delta[channel] - replaced).abs() > 0.001 {
                failures.push(format!("environment subtraction {case:?}, lane {lane}, channel {channel}: {} != {replaced}", delta[channel]));
            }
        }
    }
}

fn matrix(edge: Edge, water: bool) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut failures = Vec::new();
    for samples in crate::msaa_tests::supported_samples(&context) {
        for scale in [1_000_000, 500_000] {
            for roughness in [0.1, 0.5] {
                for full in [false, true] {
                    let case = Case {
                        size: SIZE,
                        rounding: worldrunner_render_plan::SceneDownsampleRounding::Ceil,
                        scale,
                        roughness,
                        samples,
                        edge,
                        ao: true,
                        full,
                        water,
                    };
                    check_case(&context, case, &mut failures);
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "finite receiver footprint:\n{}",
        failures.join("\n")
    );
}

#[test]
fn gpu_footprint_isolated_hit_and_zero_alpha_holes_match_finite_reference() {
    matrix(Edge::None, true);
}

#[test]
fn gpu_footprint_excludes_opaque_trace_inside_water_footprint() {
    matrix(Edge::Ownership, true);
}

#[test]
fn gpu_footprint_excludes_receiver_depth_step_inside_coarse_block() {
    matrix(Edge::Depth, true);
}

#[test]
fn gpu_footprint_excludes_receiver_normal_step_with_application_ao() {
    matrix(Edge::Normal, true);
}

#[test]
fn gpu_footprint_opaque_normal_edge_uses_real_normals_with_ao() {
    matrix(Edge::Normal, false);
}

#[test]
fn gpu_footprint_shading_normal_does_not_change_coplanar_support() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for water in [false, true] {
        for scale in [1_000_000, 500_000] {
            let case = Case {
                size: SIZE,
                rounding: worldrunner_render_plan::SceneDownsampleRounding::Ceil,
                scale,
                roughness: 0.1,
                samples: 1,
                edge: Edge::None,
                ao: false,
                full: false,
                water,
            };
            let (pairs, _) = fixture::capture_shaded(&context, case, true);
            let expected = reference(case, 0);
            let index = usize::try_from((SIZE[1] / 2 * SIZE[0] + SIZE[0] / 2) * 4).unwrap();
            for channel in 0..4 {
                assert!(
                    (pairs[0][index + channel] - expected[channel]).abs() <= 0.0005,
                    "coplanar support {case:?}: {} != {}",
                    pairs[0][index + channel],
                    expected[channel]
                );
            }
        }
    }
}

#[test]
fn footprint_shader_variants_validate_without_device() {
    for samples in [1, 4] {
        for full in [false, true] {
            for ao in [false, true] {
                let case = Case {
                    size: SIZE,
                    rounding: worldrunner_render_plan::SceneDownsampleRounding::Ceil,
                    scale: 500_000,
                    roughness: 0.1,
                    samples,
                    edge: Edge::None,
                    ao,
                    full,
                    water: true,
                };
                for pair in [false, true] {
                    let source = format!(
                        "{}\n{}",
                        include_str!("../ssao/fullscreen.wgsl"),
                        fixture::source(case, pair)
                    );
                    let module = naga::front::wgsl::parse_str(&source)
                        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
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
}

fn reflection_lod(case: Case) -> f32 {
    (case.roughness
        * gpu::scalar(u32::try_from(policy(case).radiance_extents(case.size).len() - 1).unwrap())
        - if case.scale == 500_000 { 1.0 } else { 0.0 })
    .max(0.0)
}

#[test]
fn gpu_footprint_full_size_rough_receiver_matches_existing_mip_kernel() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let case = Case {
        size: [1512, 807],
        rounding: worldrunner_render_plan::SceneDownsampleRounding::Floor,
        scale: 500_000,
        roughness: 1.0,
        samples: 1,
        edge: Edge::Ownership,
        ao: true,
        full: true,
        water: true,
    };
    assert!(reflection_lod(case) >= 6.0);
    fixture::check_mip_kernel(&context, case);
}
