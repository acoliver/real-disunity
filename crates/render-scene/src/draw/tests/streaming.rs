mod replacement;
use super::*;
use crate::offscreen::OffscreenTarget;
use worldrunner_render_plan::TerrainStreamingPolicy;

fn streamed_scene() -> RenderScene {
    let base = terrain_scene(1);
    let terrain = base
        .terrain()
        .unwrap()
        .clone()
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap()));
    base.with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}
fn resident_count(gpu: &SceneGpu) -> usize {
    gpu.terrain
        .as_ref()
        .unwrap()
        .streaming
        .as_ref()
        .unwrap()
        .committed
        .borrow()
        .len()
}
#[test]
fn chunk_uploads_are_transactional_and_forward_matches_flat_default() {
    let context = context();
    let device = context.device();
    let source = streamed_scene();
    let pipelines = ScenePipelines::new(device, OffscreenTarget::COLOR_FORMAT);
    let gpu = SceneGpu::upload(device, context.submission(), &source, &pipelines).unwrap();
    let frame = frame_from(pose());
    gpu.set_frame(context.queue(), frame);
    let abandoned = context
        .submission()
        .prepare(device, |encoder, _| {
            gpu.prepare_terrain_geometry(device, encoder, &frame, [TARGET; 2])
        })
        .unwrap();
    assert_eq!(resident_count(&gpu), 1);
    drop(abandoned);
    assert_eq!(resident_count(&gpu), 1);
    let mut target = OffscreenTarget::new(
        device,
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let streamed = target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(resident_count(&gpu), 4);
    let base =
        SceneGpu::upload(device, context.submission(), &terrain_scene(1), &pipelines).unwrap();
    base.set_frame(context.queue(), frame);
    let default = target.render(&context, &base, &pipelines).unwrap();
    assert_eq!(streamed, default);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}
#[test]
fn streaming_admits_contact_pages_and_capture_publications() {
    let context = context();
    let scene = streamed_scene();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let frame = frame_from(pose());
    assert!(
        gpu.terrain_residency_request(&frame, [TARGET; 2])
            .unwrap()
            .is_none()
    );
    assert!(gpu.capture_write_keys(context.device()).is_ok());
    let contact = worldrunner_render_plan::SceneTerrainContact::new(
        worldrunner_render_plan::SceneTint::WHITE,
        1_000_000,
        0,
        1000,
    )
    .unwrap();
    let mut materials = scene.resources().materials().to_vec();
    materials.push(materials[0].with_terrain_contact(contact));
    let resources = SceneResources::new(scene.resources().textures().to_vec(), materials).unwrap();
    let objects = scene
        .geometry()
        .objects()
        .iter()
        .map(|object| {
            SceneObject::new(
                *object.transform(),
                object.mesh(),
                MaterialHandle::from_raw(1),
            )
        })
        .collect();
    let contacted = scene
        .with_resources(
            resources,
            SceneGeometry::new(scene.geometry().meshes().to_vec(), objects).unwrap(),
            scene.effects().cloned(),
        )
        .unwrap();
    assert!(crate::draw::PreparedScene::new(&contacted).is_ok());
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}

fn streaming(gpu: &SceneGpu) -> &super::super::streaming::Streaming {
    gpu.terrain.as_ref().unwrap().streaming.as_ref().unwrap()
}

fn withheld_completion_setup(budget: u32) -> (GpuContext, SceneGpu, FrameUniform, glam::Mat4) {
    let context = context();
    let source = streamed_scene();
    let terrain = source
        .terrain()
        .unwrap()
        .clone()
        .with_streaming_policy(Some(
            TerrainStreamingPolicy::new(1, budget, 4, 1000).unwrap(),
        ));
    let source = source
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let near = frame_from(pose());
    let far_matrix = far_matrix();
    (context, gpu, near, far_matrix)
}

fn assert_iteration_counts(budget: u32, index: i32, counts: (usize, usize, usize)) {
    let (allocated, pending, retired) = counts;
    assert!(allocated + pending + retired <= budget as usize);
    assert!(pending <= 4);
    if index == 0 {
        assert_eq!((allocated, pending, retired), (1, 4, 0));
    }
    if index > 0 {
        let expected = match (budget, index) {
            (5, _) => (4, 0, 1),
            (_, 1) => (4, 1, 1),
            _ => (1, 0, 5),
        };
        assert_eq!((allocated, pending, retired), expected);
    }
}

fn withheld_completion(budget: u32) {
    let (context, gpu, mut near, far_matrix) = withheld_completion_setup(budget);
    let mut saw_retired = false;
    for index in 0..12 {
        near.view_projection = if index % 2 == 0 {
            frame_from(pose()).view_projection
        } else {
            far_matrix
        };
        let prepared = prepare_completed(&context, &gpu, &near, 0);
        assert_iteration_counts(budget, index, streaming(&gpu).allocation_counts());
        prepared.submit().unwrap();
        let (allocated, pending, retired) = streaming(&gpu).allocation_counts();
        assert!(allocated + pending + retired <= budget as usize);
        saw_retired |= retired > 0;
    }
    assert!(saw_retired);
    wait(&context);
    let completed = context.submission().completed_serial();
    assert!(completed > 0);
    near.view_projection = far_matrix;
    let prepared = prepare_completed(&context, &gpu, &near, completed);
    assert_eq!(
        streaming(&gpu).allocation_counts(),
        if budget == 5 { (4, 1, 0) } else { (1, 0, 0) }
    );
    prepared.submit().unwrap();
    assert_eq!(resident_count(&gpu), 1);
}

#[test]
fn retired_and_pending_allocations_hold_the_budget_until_completion() {
    withheld_completion(5);
    withheld_completion(9);
}

#[test]
fn compositor_entry_points_reuse_one_preparation_and_reject_a_second_view() {
    let context = context();
    let source = streamed_scene();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let target = target(&context);
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let frame = frame_from(pose());
    gpu.set_frame(context.queue(), frame);
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            crate::compositor::encode_scene(
                &mut renderer,
                context.device(),
                encoder,
                (&mut gpu, &pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
                frame,
            )?;
            let buffers = streaming(&gpu).prepared_buffers();
            crate::compositor::encode_scene(
                &mut renderer,
                context.device(),
                encoder,
                (&mut gpu, &pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
                frame,
            )?;
            crate::compositor::encode_resident(
                &mut renderer,
                context.device(),
                encoder,
                (&gpu, &pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
            )?;
            assert_eq!(streaming(&gpu).prepared_buffers(), buffers);
            assert_eq!(streaming(&gpu).allocation_counts(), (1, 4, 0));
            let mut changed = frame;
            changed.view_projection *= glam::Mat4::from_scale(glam::Vec3::splat(2.0));
            assert_eq!(
                crate::compositor::encode_scene(
                    &mut renderer,
                    context.device(),
                    encoder,
                    (&mut gpu, &pipelines),
                    (&target.color_attachment(), &target.depth_attachment()),
                    changed
                ),
                Err(worldrunner_render_post::FrameError::StreamingSelectionConflict)
            );
            assert_eq!(streaming(&gpu).prepared_buffers(), buffers);
            assert_eq!(streaming(&gpu).allocation_counts(), (1, 4, 0));
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap();
    prepared.cancel();
    assert_eq!(resident_count(&gpu), 1);
}

#[test]
fn streaming_allows_non_camera_resident_edits_without_replacing_chunks() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = streamed_scene();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    let object = gpu.instance(handle).unwrap().0;
    let buffer = streaming(&gpu).committed.borrow()[0].1.vertices.clone();
    let added = gpu.add_instance(object, false).unwrap();
    gpu.set_instance_transform(handle, *object.transform())
        .unwrap();
    gpu.set_instance_object(handle, object).unwrap();
    gpu.set_instance_visible(handle, false).unwrap();
    gpu.remove_instance(added).unwrap();
    gpu.set_post_settings(&SceneEffects::new(effect_parts()).unwrap())
        .unwrap();
    assert_eq!(streaming(&gpu).committed.borrow()[0].1.vertices, buffer);
    wait(&context);
}

#[test]
fn geometry_dependency_waits_for_submitted_chunks_and_rejects_other_sources() {
    let (context, gpu, frame, _) = withheld_completion_setup(9);
    let request = gpu.terrain_geometry_request(&frame, [TARGET; 2]).unwrap();
    assert!(!gpu.terrain_geometry_ready(&request).unwrap());
    let cancelled = prepare_completed(&context, &gpu, &frame, 0);
    assert!(!gpu.terrain_geometry_ready(&request).unwrap());
    cancelled.cancel();
    assert!(!gpu.terrain_geometry_ready(&request).unwrap());
    prepare_completed(&context, &gpu, &frame, 0)
        .submit()
        .unwrap();
    assert!(gpu.terrain_geometry_ready(&request).unwrap());
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let other = SceneGpu::upload(
        context.device(),
        context.submission(),
        &streamed_scene(),
        &pipelines,
    )
    .unwrap();
    assert!(other.terrain_geometry_ready(&request).is_err());
    wait(&context);
}

fn far_matrix() -> glam::Mat4 {
    glam::camera::rh::proj::directx::orthographic(-10000.0, 10000.0, -10000.0, 10000.0, 0.1, 1000.0)
        * glam::camera::rh::view::look_at_mat4(
            glam::Vec3::new(0.0, 0.0, 100.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        )
}

fn prepare_completed(
    context: &GpuContext,
    gpu: &SceneGpu,
    frame: &FrameUniform,
    completed: u64,
) -> worldrunner_render_gpu::submission::PreparedFrame {
    context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let mut upload =
                worldrunner_render_post::PrepareContext::new(context.device(), encoder)?;
            streaming(gpu).prepare_completed(&mut upload, frame, [TARGET; 2], completed)?;
            upload.finish();
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap()
}

fn target(context: &GpuContext) -> OffscreenTarget {
    OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap()
}

fn wait(context: &GpuContext) {
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}
