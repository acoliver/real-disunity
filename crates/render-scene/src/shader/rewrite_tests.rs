//! Variant assemblies rewrite their base at the anchors they name, and a base reworded at an
//! anchor stops the assembly instead of returning the base unchanged.

use worldrunner_render_plan::{SceneCelestialLighting, SceneSsaoReceiver};

const CENTRE_SURFACE: &str =
    "let centre_surface = read_surface_at(i32(round(grid.x)), i32(round(grid.y)));";
const BLENDED: &str =
    "let blended = blend_surfaces(grid, world_xy, world_xy_dx, world_xy_dy, centre_surface);";

fn reworded(source: &str, anchor: &str) -> String {
    assert_eq!(source.matches(anchor).count(), 1, "fixture anchor {anchor}");
    source.replace(anchor, &anchor.replace(' ', "  "))
}

#[test]
fn cached_terrain_pages_replace_surface_selection_and_blend() {
    let base = crate::shader::TERRAIN_WGSL;
    let cached = crate::terrain::program::source::cached_source(base);
    assert!(!cached.contains(CENTRE_SURFACE));
    assert!(!cached.contains(BLENDED));
    assert_eq!(cached.matches("let centre_surface = 0u;").count(), 1);
    assert_eq!(
        cached
            .matches("let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);")
            .count(),
        1
    );
    let expected = base
        .replacen(CENTRE_SURFACE, "let centre_surface = 0u;", 1)
        .replacen(
            BLENDED,
            "let blended = cached_surface(world_xy, world_xy_dx, world_xy_dy);",
            1,
        );
    assert_eq!(
        cached.strip_suffix(include_str!("../terrain/pages/sampling.wgsl")),
        Some(expected.as_str())
    );
}

#[test]
#[should_panic(expected = "terrain::program::source::cached_source")]
fn cached_terrain_pages_reject_a_reworded_surface_selection() {
    let _ = crate::terrain::program::source::cached_source(&reworded(
        crate::shader::TERRAIN_WGSL,
        CENTRE_SURFACE,
    ));
}

#[test]
#[should_panic(expected = "let blended = blend_surfaces(grid")]
fn cached_terrain_pages_reject_a_reworded_blend() {
    let _ = crate::terrain::program::source::cached_source(&reworded(
        crate::shader::TERRAIN_WGSL,
        BLENDED,
    ));
}

#[test]
fn anchored_celestial_vertex_drops_the_camera_offset() {
    for composition in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let lighting = SceneCelestialLighting::NEUTRAL;
        let free = crate::renderer::celestial_source(composition, lighting, false);
        let anchored = crate::renderer::celestial_source(composition, lighting, true);
        let offset = "instance.model_column_3.xyz+frame.camera_position.xyz";
        assert_eq!(free.matches(offset).count(), 1);
        assert!(!anchored.contains(offset));
        assert_eq!(
            anchored,
            free.replacen(offset, "instance.model_column_3.xyz", 1)
        );
        assert!(
            free.contains(
                "@location(0) @interpolate(perspective, sample) world_position: vec3<f32>,"
            )
        );
    }
}

#[test]
fn depth_aware_gather_replaces_the_nearest_receiver_and_both_compositions() {
    for base in [crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL] {
        let nearest = crate::ssao::ambient_source(base);
        let gather = crate::ssao::receiver::source(&nearest, SceneSsaoReceiver::DepthAwareGather);
        for (anchor, replacement) in [
            (
                "return material_ao * screen;",
                "return min(1.01 * screen, material_ao);",
            ),
            (
                "ambient_occlusion * scene_screen_ao(world_position)",
                "min(1.01 * scene_screen_ao(world_position), ambient_occlusion)",
            ),
            (
                "return textureLoad(ambient_texture, clamp(",
                "return receiver_gather(uv, -(ao_receiver.view * vec4<f32>(world, 1.0)).z);",
            ),
        ] {
            assert_eq!(nearest.matches(anchor).count(), 1, "{anchor}");
            assert!(!gather.contains(anchor), "{anchor}");
            assert_eq!(gather.matches(replacement).count(), 1, "{replacement}");
        }
    }
}

#[test]
#[should_panic(expected = "anchor \"return material_ao * screen;\" matched 0 times")]
fn depth_aware_gather_rejects_a_reworded_specular_occlusion() {
    let nearest = crate::ssao::ambient_source(crate::shader::TERRAIN_WGSL);
    let _ = crate::ssao::receiver::source(
        &reworded(&nearest, "return material_ao * screen;"),
        SceneSsaoReceiver::DepthAwareGather,
    );
}

#[test]
#[should_panic(
    expected = "anchor \"ambient_occlusion * scene_screen_ao(world_position)\" matched 0 times"
)]
fn depth_aware_gather_rejects_a_reworded_ambient_composition() {
    let nearest = crate::ssao::ambient_source(crate::shader::TERRAIN_WGSL);
    let _ = crate::ssao::receiver::source(
        &reworded(
            &nearest,
            "ambient_occlusion * scene_screen_ao(world_position)",
        ),
        SceneSsaoReceiver::DepthAwareGather,
    );
}

#[test]
fn velocity_object_source_rewrites_every_stock_fragment_entry() {
    let source = crate::velocity::object_source(crate::shader::ENVIRONMENT_WGSL);
    for name in [
        "fs_opaque",
        "fs_masked",
        "fs_relief_opaque",
        "fs_relief_masked",
    ] {
        assert!(
            !source.contains(&format!("@fragment\nfn {name}(")),
            "{name}"
        );
        assert_eq!(
            source.matches(&format!("@fragment fn {name}(")).count(),
            1,
            "{name}"
        );
    }
}

#[test]
#[should_panic(
    expected = "shader rewrite of velocity::vertex_source: anchor \"@vertex fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {\" matched 0 times"
)]
fn velocity_vertex_source_rejects_a_reworded_vertex_entry() {
    let _ = crate::velocity::vertex_source(&reworded(
        crate::shader::ENVIRONMENT_WGSL,
        "@vertex fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {",
    ));
}
