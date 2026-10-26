#[test]
fn radiance_limit_boundaries_include_sample_packing_and_odd_half_extents() {
    let limits = wgpu::Limits {
        max_texture_dimension_2d: 8192,
        ..Default::default()
    };
    for (scale, accepted, rejected) in [(1_000_000, 2048, 2049), (500_000, 4096, 4097)] {
        let settings = SceneSsr {
            resolution_scale_micro: scale,
            ..settings()
        };
        for (width, valid) in [(accepted, true), (rejected, false), (u32::MAX, false)] {
            let config = FrameConfig::new([width, 64], wgpu::TextureFormat::Rgba16Float, 4);
            assert_eq!(validate_extent(config, &settings, &limits).is_ok(), valid);
        }
    }
}

use super::*;

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
