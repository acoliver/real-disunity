//! Numeric image deltas for the selected-plane terrain receiver against hardware dP.
use super::*;

fn comparison_pipelines(
    context: &GpuContext,
    renderer: &ShadowedScene,
    terrain: &str,
) -> ScenePipelines {
    ScenePipelines::with_screen_space(
        context.device(),
        (OffscreenTarget::COLOR_FORMAT, 1),
        resources::SHADOWED_WGSL,
        terrain,
        Some(&renderer.shadow_layout),
        None,
    )
}

fn seam_probe(source: &str) -> String {
    let body = crate::scene_metadata::receiver::function_source(source, "fs_terrain");
    source.replace(body, "fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> { return vec4<f32>(normalize(terrain_face_normal(input.world_position.xy, input.plane_index, input.skirt_axis)) * 0.5 + 0.5, 1.0); }")
}

fn quad_seam(image: &RgbaImage, pixel: usize) -> bool {
    let width = image.viewport.width as usize;
    let height = image.viewport.height as usize;
    let (x, y) = (pixel % width, pixel / width);
    let sample = &image.pixels[pixel * 4..pixel * 4 + 3];
    [(x ^ 1, y), (x, y ^ 1), (x ^ 1, y ^ 1)]
        .into_iter()
        .filter(|&(x, y)| x < width && y < height)
        .any(|(x, y)| sample != &image.pixels[(y * width + x) * 4..(y * width + x) * 4 + 3])
}

fn compare(context: &GpuContext, source: &RenderScene, name: &str, cascades: usize) {
    let mut renderer = ShadowedScene::new(context, source, settings()).unwrap();
    let selected = resources::shadowed_terrain_source();
    let hardware = crate::scene_metadata::receiver::hardware_receiver_reference(selected);
    assert_ne!(selected, hardware);
    let mut reference = comparison_pipelines(context, &renderer, &hardware);
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, context, &view(), &target).unwrap();
    let actual = target.read_rgba8(context).unwrap();
    std::mem::swap(&mut renderer.pipelines, &mut reference);
    render_target(&mut renderer, context, &view(), &target).unwrap();
    let main = target.read_rgba8(context).unwrap();
    renderer.pipelines = comparison_pipelines(context, &renderer, &seam_probe(selected));
    render_target(&mut renderer, context, &view(), &target).unwrap();
    let planes = target.read_rgba8(context).unwrap();
    let mut changed = 0;
    let mut at_seams = 0;
    let mut maximum = 0;
    for (pixel, (a, b)) in actual
        .pixels
        .chunks_exact(4)
        .zip(main.pixels.chunks_exact(4))
        .enumerate()
    {
        maximum = maximum.max(a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap());
        if a != b {
            changed += 1;
            at_seams += usize::from(quad_seam(&planes, pixel));
        }
    }
    let seam_pixels = (0..planes.pixels.len() / 4)
        .filter(|&pixel| quad_seam(&planes, pixel))
        .count();
    println!(
        "TERRAIN_RECEIVER_IMAGE name={name} cascades={cascades} changed_pixels={changed} quad_seam_pixels={seam_pixels} changed_on_seams={at_seams} changed_off_seams={} max_channel={maximum}",
        changed - at_seams
    );
}

#[test]
fn gpu_terrain_receiver_reports_shadow_pixel_changes_and_selected_plane_quad_seams() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let base = scene(&[[0, 0, 1_000]], 600_000, None);
    for (name, ridge) in [("terrain", false), ("ridge", true)] {
        for extents in [vec![4_000], vec![2_000, 4_000, 8_000]] {
            let source = super::cascades::source_with(
                &super::terrain::terrain_scene(&base, ridge, false),
                SceneShadowCascades::new(extents.clone(), 100_000).unwrap(),
            );
            compare(&context, &source, name, extents.len());
        }
    }
}
