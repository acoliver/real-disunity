use super::*;
use crate::draw::TerrainGeometryStatistics;

fn source(budget: u32) -> RenderScene {
    let base = streamed_scene();
    let terrain =
        worldrunner_render_plan::SceneTerrain::new(worldrunner_render_plan::SceneTerrainParts {
            columns: 65,
            rows: 65,
            origin_east_millimeters: 0,
            origin_north_millimeters: 0,
            sample_spacing_millimeters: 1000,
            heights_millimeters: vec![0; 65 * 65],
            surface_indices: vec![0; 65 * 65],
            surfaces: base.terrain().unwrap().surfaces().to_vec(),
            lod_policy: worldrunner_render_plan::TerrainLodPolicy::FullResolution,
        })
        .unwrap()
        .with_streaming_policy(Some(
            TerrainStreamingPolicy::new(8, budget, 4, 1000).unwrap(),
        ));
    base.with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}

fn near_frame() -> FrameUniform {
    let mut frame = frame_from(pose());
    frame.view_projection =
        glam::camera::rh::proj::directx::orthographic(-4.0, 4.0, -4.0, 4.0, 0.1, 1000.0)
            * glam::camera::rh::view::look_at_mat4(
                glam::Vec3::new(32.0, 32.0, 100.0),
                glam::Vec3::new(32.0, 32.0, 0.0),
                glam::Vec3::Y,
            );
    frame
}

fn observed(gpu: &SceneGpu) -> crate::draw::TerrainStreamingStatistics {
    let Some(TerrainGeometryStatistics::Streaming(stats)) = gpu.statistics().terrain_geometry
    else {
        panic!("submitted streaming terrain");
    };
    stats
}

#[test]
fn geometry_observations_publish_streaming_then_settled_and_camera_leaf_identity() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &source(1000),
        &pipelines,
    )
    .unwrap();
    let mut target = target(&context);
    gpu.set_frame(context.queue(), near_frame());
    assert!(gpu.statistics().terrain_geometry.is_none());
    target.render(&context, &gpu, &pipelines).unwrap();
    let first = observed(&gpu);
    assert!(!first.complete);
    assert!(first.unresolved_leaves > 0);
    assert!(first.drawn_coarse > 0);
    for _ in 0..32 {
        target.render(&context, &gpu, &pipelines).unwrap();
    }
    let settled = observed(&gpu);
    assert!(settled.complete);
    assert_eq!(settled.selected_leaves, settled.drawn_at_detail);
    target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(observed(&gpu).drawn, settled.drawn);
    let mut far = near_frame();
    far.view_projection = far_matrix();
    gpu.set_frame(context.queue(), far);
    for _ in 0..4 {
        target.render(&context, &gpu, &pipelines).unwrap();
    }
    assert_ne!(observed(&gpu).drawn, settled.drawn);
    wait(&context);
}

#[test]
fn geometry_observations_budget_limit_and_cancellation_preserve_submission() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &source(9),
        &pipelines,
    )
    .unwrap();
    let mut target = target(&context);
    let frame = near_frame();
    gpu.set_frame(context.queue(), frame);
    target.render(&context, &gpu, &pipelines).unwrap();
    let previous = gpu.statistics().terrain_geometry;
    assert!(!observed(&gpu).complete);
    assert!(observed(&gpu).drawn_coarse > 0);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let mut encoder = context.submission().begin(context.device()).unwrap();
    crate::compositor::encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&target.color_attachment(), &target.depth_attachment()),
        frame,
    )
    .unwrap();
    assert_eq!(gpu.statistics().terrain_geometry, previous);
    drop(encoder);
    assert_eq!(gpu.statistics().terrain_geometry, previous);
    wait(&context);
}
