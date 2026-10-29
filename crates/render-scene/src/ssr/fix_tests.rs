use super::*;

const PACKED_EXTENT_ERROR: &str = "sample-packed radiance extent exceeds device texture limit";

#[test]
fn radiance_limit_boundaries_include_sample_packing_and_odd_half_extents() {
    let limits = wgpu::Limits {
        max_texture_dimension_2d: 8192,
        ..Default::default()
    };
    for samples in crate::msaa_tests::all_counts() {
        // Samples are packed side by side, so the widest frame that fits shrinks as the count grows.
        let fit = limits.max_texture_dimension_2d / samples;
        for (scale, accepted, rejected) in
            [(1_000_000, fit, fit + 1), (500_000, fit * 2, fit * 2 + 1)]
        {
            let settings = SceneSsr {
                resolution_scale_micro: scale,
                ..settings()
            };
            for (width, valid) in [(accepted, true), (rejected, false), (u32::MAX, false)] {
                let config =
                    FrameConfig::new([width, 64], wgpu::TextureFormat::Rgba16Float, samples);
                let result = validate_extent(config, &settings, &limits);
                assert_eq!(
                    result.is_ok(),
                    valid,
                    "{samples}x width {width} scale {scale}: {result:?}"
                );
                if !valid {
                    assert!(
                        matches!(result, Err(FrameError::SsrSettings(PACKED_EXTENT_ERROR))),
                        "{samples}x width {width} scale {scale}: {result:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn frame_wider_than_the_device_limit_divided_by_samples_fails_before_ssr_allocation() {
    let context = context();
    let limit = context.device().limits().max_texture_dimension_2d;
    let effects = crate::post_fxaa_tests::effects(None)
        .with_ssr(settings())
        .unwrap();
    let build = |width: u32, samples: u32| {
        crate::compositor::FrameRenderer::hdr(
            &context,
            [width, 4],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
            worldrunner_render_post::ToneMap::Reinhard,
            Some(&effects),
        )
    };
    let mut rejected = Vec::new();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let scope = context
            .device()
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let width = limit / samples + 1;
        let result = build(width, samples);
        assert!(
            matches!(
                result,
                Err(worldrunner_render_post::PostError::Frame(
                    FrameError::SsrSettings(PACKED_EXTENT_ERROR)
                ))
            ),
            "{samples}x width {width} against limit {limit}: {:?}",
            result.err()
        );
        assert!(
            pollster::block_on(scope.pop()).is_none(),
            "{samples}x: rejection must precede any GPU work"
        );
        if samples > 1 {
            // The widest frame whose lanes fit is accepted.
            assert!(
                build(limit / samples, samples).is_ok(),
                "{samples}x width {} fits {limit}",
                limit / samples
            );
        }
        rejected.push(samples);
    }
    assert!(rejected.contains(&4), "4x is exercised on every adapter");
}

#[test]
fn water_origin_uses_the_published_forward_sample_offset() {
    assert!(include_str!("../water/water.wgsl").contains("var f0 = 0.02037;"));
    let source = gpu::trace_source(4, true);
    assert!(source.contains("origin_uv = (vec2<f32>(p) + water_surface.rg) / camera.size.xy"));
    assert!(
        include_str!("../water/water.wgsl")
            .contains("receiver_pixel - floor(input.position.xy), f0")
    );
}
#[test]
fn target_ownership_does_not_infer_a_plane_from_shading_normals() {
    let source = gpu::trace_source(4, true);
    assert!(!source.contains("let plane = dot(centre, normal)"));
    assert!(source.contains("textureLoad(opaque_forward_depth, pixel, 0)"));
    assert!(source.contains("textureLoad(water_normals, pixel, lane).a != 0.0"));
}
#[test]
fn packed_extents_are_checked_before_ssr_allocation() {
    let source = include_str!("../ssr.rs");
    let constructor = source.split("pub fn new(").nth(1).unwrap();
    assert!(
        constructor
            .find("validate_extent(config")
            .unwrap_or(usize::MAX)
            < constructor.find("forward::install(").unwrap()
    );
}
#[test]
fn water_trace_and_resolve_explicitly_handle_orthographic_projection() {
    for source in [
        gpu::trace_source(4, true),
        gpu::sampled_resolve_source(3, 4, true),
    ] {
        assert!(source.contains("camera.projection[3][3] != 0.0"));
        assert!(source.contains("reflection_incident("));
    }
}
