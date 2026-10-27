use super::*;

fn edited_scene(source: &RenderScene, edits: &[(usize, i32)]) -> RenderScene {
    let base = source.terrain().unwrap();
    let mut heights = base.heights_millimeters().to_vec();
    for &(index, height) in edits {
        heights[index] = height;
    }
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: base.columns(),
        rows: base.rows(),
        origin_east_millimeters: base.origin_east_millimeters(),
        origin_north_millimeters: base.origin_north_millimeters(),
        sample_spacing_millimeters: base.sample_spacing_millimeters(),
        heights_millimeters: heights,
        surface_indices: base.surface_indices().to_vec(),
        surfaces: base.surfaces().to_vec(),
        lod_policy: base.lod_policy(),
    })
    .unwrap()
    .with_streaming_policy(base.streaming_policy());
    source
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}
fn source() -> RenderScene {
    let base = terrain_scene(1);
    let mut heights = vec![-500; 81];
    heights[80] = 500;
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 9,
        rows: 9,
        origin_east_millimeters: -4000,
        origin_north_millimeters: -4000,
        sample_spacing_millimeters: 1000,
        heights_millimeters: heights,
        surface_indices: vec![0; 81],
        surfaces: base.terrain().unwrap().surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(TerrainStreamingPolicy::new(4, 9, 4, 1000).unwrap()));
    base.with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}
fn advance(context: &GpuContext, gpu: &SceneGpu, submit: bool) -> bool {
    let mut ready = false;
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let mut upload =
                worldrunner_render_post::PrepareContext::new(context.device(), encoder)?;
            ready = gpu.prepare_retained_terrain(&mut upload)?;
            upload.finish();
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap();
    if submit {
        prepared.submit().unwrap();
    } else {
        prepared.cancel();
    }
    ready
}
fn check_replacement(edits: &[(usize, i32)], expected: usize) {
    let context = context();
    let source = source();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let old =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let frame = frame_from(pose());
    prepare_completed(&context, &old, &frame, 0)
        .submit()
        .unwrap();
    wait(&context);
    assert_eq!(resident_count(&old), 4);
    let request = old.terrain_geometry_request(&frame, [TARGET; 2]).unwrap();
    let next = SceneGpu::upload_prepared_reusing(
        context.device(),
        context.submission(),
        super::super::super::PreparedScene::new(&edited_scene(&source, edits)).unwrap(),
        &pipelines,
        Some(&old),
    )
    .unwrap();
    assert_eq!(resident_count(&next), 4 - expected);
    assert!(matches!(
        next.terrain_geometry_ready(&request),
        Err(crate::terrain::pages::scene::TerrainResidencyError::Stale)
    ));
    assert!(!advance(&context, &next, false));
    assert_eq!(resident_count(&next), 4 - expected);
    assert!(!advance(&context, &next, true));
    assert_eq!(resident_count(&next), 4);
    assert!(advance(&context, &next, true));
    next.finish_retained_terrain();
    let old_buffers = streaming(&old).committed.borrow();
    let next_buffers = streaming(&next).committed.borrow();
    let changed = next_buffers
        .iter()
        .filter(|(_, next)| {
            !old_buffers
                .iter()
                .any(|(_, old)| old.vertices == next.vertices)
        })
        .count();
    assert_eq!(changed, expected);
    assert!(old_buffers.len() + changed <= 9);
    assert!(changed <= 4);
    wait(&context);
}
#[test]
fn local_height_edit_only_uploads_the_touching_source_span() {
    check_replacement(&[(10, 0)], 1);
}
#[test]
fn shared_stitch_boundary_invalidates_every_touching_chunk() {
    check_replacement(&[(40, 0)], 4);
}
#[test]
fn changed_elevation_range_rebuilds_all_resident_chunks() {
    check_replacement(&[(80, 1000)], 4);
}

#[test]
fn tight_budget_coarsens_the_previous_source_before_rebuilding() {
    let context = context();
    let initial = source();
    let terrain = initial
        .terrain()
        .unwrap()
        .clone()
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(4, 5, 4, 1000).unwrap()));
    let source = initial
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let old =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    prepare_completed(&context, &old, &frame_from(pose()), 0)
        .submit()
        .unwrap();
    wait(&context);
    let next = SceneGpu::upload_prepared_reusing(
        context.device(),
        context.submission(),
        super::super::super::PreparedScene::new(&edited_scene(&source, &[(80, 1000)])).unwrap(),
        &pipelines,
        Some(&old),
    )
    .unwrap();
    assert!(!advance(&context, &next, true));
    assert_eq!(resident_count(&old), 1);
    assert_eq!(resident_count(&next), 0);
    wait(&context);
    assert!(!advance(&context, &next, true));
    assert_eq!(resident_count(&old) + resident_count(&next), 5);
    assert!(next.retained_terrain_ready());
    next.finish_retained_terrain();
    wait(&context);
}
