use super::*;
use worldrunner_render_plan::SceneDownsampleRounding;

fn case(size: [u32; 2], edge: Edge, rounding: SceneDownsampleRounding) -> Case {
    Case {
        size,
        scale: 1_000_000,
        roughness: 1.0,
        samples: 1,
        edge,
        ao: true,
        full: true,
        water: true,
        rounding,
    }
}

#[test]
fn gpu_recursive_partial_bins_use_existing_mip_kernel() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for rounding in [
        SceneDownsampleRounding::Ceil,
        SceneDownsampleRounding::Floor,
    ] {
        for edge in [Edge::None, Edge::Normal] {
            let case = case([5, 1], edge, rounding);
            assert!(reflection_lod(case) >= 2.0);
            fixture::check_mip_kernel(&context, case);
        }
    }
}

#[test]
fn gpu_odd_extent_bilinear_levels_use_existing_mip_kernel() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for edge in [Edge::None, Edge::Normal] {
        for level in [2, 3] {
            let mut case = case(SIZE, edge, SceneDownsampleRounding::Ceil);
            let levels = u32::try_from(policy(case).radiance_extents(case.size).len() - 1).unwrap();
            case.roughness = (gpu::scalar(level) + 0.25) / gpu::scalar(levels);
            assert!(reflection_lod(case) > gpu::scalar(level));
            fixture::check_mip_kernel(&context, case);
        }
    }
}
