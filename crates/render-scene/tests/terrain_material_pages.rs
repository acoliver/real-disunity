use worldrunner_render_scene::{
    device::GpuContext,
    draw::SceneGpu,
    renderer::ScenePipelines,
    view::{CameraPose, SceneView, Viewport},
};
#[path = "terrain_material_pages/fixture.rs"]
mod fixture;
#[path = "terrain_material_pages/stock.rs"]
mod stock;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{TerrainPageSettings, TerrainPageSettingsParts};
use worldrunner_render_scene::offscreen::OffscreenTarget;
use worldrunner_render_scene::terrain::pages::{
    composition::LayerSample, scene::TerrainPageSource,
};

fn page_settings(interior: [u32; 2]) -> TerrainPageSettings {
    let mut parts = TerrainPageSettingsParts {
        interior,
        border: 2,
        filter_radius: 1,
        maximum_depth: 1,
        split_threshold_millipixels: 100,
        merge_threshold_millipixels: 50,
        maximum_selected_pages: 4,
        resident_bytes: 100_000,
        upload_bytes_per_frame: 100_000,
    };
    parts.upload_bytes_per_frame = TerrainPageSettings::new(parts).unwrap().page_bytes();
    TerrainPageSettings::new(parts).unwrap()
}

#[test]
fn terrain_pages_capture_waits_and_two_configurations_render_exact_border_bytes() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let view = page_view();
    for (interior, color) in [([4, 4], [64, 128, 192, 255]), ([7, 3], [192, 64, 128, 255])] {
        let source = fixture::scene(interior != [4, 4]);
        let mut gpu =
            SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
        let settings = page_settings(interior);
        gpu.enable_terrain_pages(
            context.device(),
            context.submission(),
            &pipelines,
            TerrainPageSource {
                settings,
                slots: 5,
                budget: AllocationBudget(1_000_000),
                sample: Box::new(move |_| {
                    vec![LayerSample {
                        color,
                        normal: [0, 0, i16::MAX],
                        roughness: 180,
                        weight: 1,
                    }]
                }),
            },
        )
        .unwrap();
        gpu.set_frame(
            context.queue(),
            worldrunner_render_scene::frame::FrameUniform::unlit(&view),
        );
        assert!(!gpu.terrain_pages_ready());
        let frame = worldrunner_render_scene::frame::FrameUniform::unlit(&view);
        let request = gpu
            .terrain_residency_request(&frame, [view.viewport().width, view.viewport().height])
            .unwrap()
            .unwrap();
        let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
        let image = target.render(&context, &gpu, &pipelines).unwrap();
        assert!(gpu.terrain_pages_ready());
        assert_eq!(gpu.terrain_residency_ready(&request), Ok(true));
        assert_uniform_page_colour(
            &image.pixels[40 * 4..41 * 4],
            color,
            "four-page intersection",
        );
        assert_uniform_page_colour(&image.pixels[39 * 4..40 * 4], color, "left page gutter");
        gpu.invalidate_terrain_pages([0.4, 0.4], [0.6, 0.6]);
        assert_eq!(
            gpu.terrain_residency_ready(&request),
            Err(worldrunner_render_scene::terrain::pages::scene::TerrainResidencyError::Stale)
        );
        assert!(!gpu.terrain_pages_ready());
        assert_eq!(target.render(&context, &gpu, &pipelines).unwrap(), image);
    }
}

/// The page texels all hold one colour, so a bilinear filter across a page intersection or gutter
/// returns that texel value for any weights (the weights sum to one) and the display value is the
/// sRGB encoding of the UNORM texel. Rounding is bounded by the shared output bound; a 0 or 255
/// alpha leaves no room.
fn assert_uniform_page_colour(pixel: &[u8], texel: [u8; 4], context: &str) {
    use worldrunner_render_gpu::tolerance_test_support::{assert_code_within, encoded_steps};
    for channel in 0..3 {
        assert_code_within(
            pixel[channel],
            encoded_steps(f64::from(texel[channel]) / 255.0, true),
            0.0,
            &format!("{context}, channel {channel}"),
        );
    }
    assert_eq!(pixel[3], texel[3], "{context}, alpha");
}

fn page_view() -> SceneView {
    let viewport = Viewport {
        width: 9,
        height: 9,
    };
    SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [0, 0, 5000],
            target_millimeters: [0, 0, 0],
        },
        worldrunner_render_scene::view::OrthographicLens {
            vertical_half_extent_meters: 2.0,
            near_meters: 0.1,
            far_meters: 10.0,
        },
        [0.0, 1.0, 0.0],
        viewport,
    )
    .unwrap()
}
