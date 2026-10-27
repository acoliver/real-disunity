use super::*;

fn policy() -> worldrunner_render_shadow::CascadeRefreshPolicy {
    worldrunner_render_shadow::CascadeRefreshPolicy {
        intervals: [8; 4],
        phases: [0; 4],
        max_non_near: 3,
        light_delta_radians: 0.1,
        projection_delta_texels: 4.0,
    }
}

#[test]
fn gpu_static_refresh_policy_reuses_maps_and_visibility() {
    let context = context();
    let source = cascades::source_with(
        &scene(&[[0, 0, 1_000]], 600_000, None),
        SceneShadowCascades::new(vec![1_000, 2_000, 3_000, 4_000], 0).unwrap(),
    );
    let mut renderer = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            refresh_policy: policy(),
            ..settings()
        },
    )
    .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let fresh = target.read_rgba8(&context).unwrap();
    let maps = depth_layers(&context, &renderer);
    let before = crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(std::mem::take);
    assert!(before.contains(&PassSlot::ShadowCasters(3)));
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let reused = target.read_rgba8(&context).unwrap();
    let passes = crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(std::mem::take);
    assert!(
        !passes
            .iter()
            .any(|slot| matches!(slot, PassSlot::ShadowCasters(_)))
    );
    assert_eq!(fresh, reused);
    assert_eq!(maps, depth_layers(&context, &renderer));
    let fresh_default = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    assert_eq!(fresh_default, reused);
}

pub(super) fn depth_layers(context: &GpuContext, renderer: &ShadowedScene) -> Vec<Vec<f32>> {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let copies: Vec<_> = (0..renderer.map.depth.layers())
        .map(|layer| {
            renderer
                .map
                .depth
                .copy_layer_to_readback(context.device(), &mut encoder, layer)
                .unwrap()
        })
        .collect();
    encoder.finish().submit().unwrap();
    copies
        .iter()
        .map(|copy| {
            let (send, receive) = std::sync::mpsc::channel();
            copy.buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    send.send(result).unwrap();
                });
            context
                .device()
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(30)),
                })
                .unwrap();
            receive
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
                .unwrap();
            let mapped = copy.buffer.slice(..).get_mapped_range().unwrap();
            let values = mapped
                .chunks_exact(usize::try_from(copy.bytes_per_row).unwrap())
                .flat_map(|row| {
                    row[..usize::try_from(copy.resolution).unwrap() * size_of::<f32>()]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|bytes| f32::from_le_bytes(*bytes))
                })
                .collect();
            drop(mapped);
            copy.buffer.unmap();
            values
        })
        .collect()
}

#[test]
fn gpu_refresh_invalidation_capture_and_abandonment() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            refresh_policy: policy(),
            ..settings()
        },
    )
    .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(renderer.cascade_refresh_states()[0].last_frame(), Some(0));
    renderer.request_shadow_capture();
    let mut abandoned = context.submission().begin(context.device()).unwrap();
    renderer
        .encode(
            &context,
            &view(),
            &target.color_attachment(),
            &target.depth_attachment(),
            &mut abandoned,
        )
        .unwrap();
    drop(abandoned);
    assert_eq!(renderer.cascade_refresh_states()[0].last_frame(), Some(0));
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(worldrunner_render_shadow::RefreshReason::Capture)
    );
    let handle = renderer.scene_mut().instance_handles().nth(1).unwrap();
    renderer
        .scene_mut()
        .set_instance_visible(handle, false)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(worldrunner_render_shadow::RefreshReason::Generation)
    );
    renderer
        .set_settings(
            &context,
            DirectionalShadowSettings {
                resolution: 256,
                refresh_policy: policy(),
                ..settings()
            },
        )
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(renderer.cascade_refresh_states()[0].last_frame(), Some(3));
    renderer.invalidate_shadows();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(renderer.cascade_refresh_states()[0].last_frame(), Some(4));
}

#[test]
fn gpu_camera_motion_refreshes_and_uploads_map_consistent_matrices() {
    let context = context();
    let source = cascades::source_with(
        &scene(&[[0, 0, 1_000]], 600_000, None),
        SceneShadowCascades::camera_splits(vec![2_000, 5_000, 10_000, 20_000], 0).unwrap(),
    );
    let mut renderer = ShadowedScene::new(
        &context,
        &source,
        DirectionalShadowSettings {
            refresh_policy: policy(),
            ..settings()
        },
    )
    .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let initial = renderer.cascade_refresh_states();
    for (frame, shift) in [(1, 100), (2, 2_000)] {
        let moved = translated_view(shift, view().viewport());
        render_target(&mut renderer, &context, &moved, &target).unwrap();
        let states = renderer.cascade_refresh_states();
        if frame == 1 {
            assert_eq!(states[3].matrix(), initial[3].matrix());
        } else {
            assert_eq!(states[3].last_frame(), Some(frame));
            assert_ne!(states[3].matrix(), initial[3].matrix());
        }
        let uniforms = super::scheduler::uniforms(&context, &renderer);
        let values: Vec<_> = uniforms[1]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f32::from_le_bytes(*bytes))
            .collect();
        for (index, state) in states.iter().enumerate() {
            assert_eq!(
                &values[index * 16..(index + 1) * 16],
                &state.matrix().unwrap().to_cols_array()
            );
        }
    }
}

#[test]
fn gpu_sun_depth_settings_and_resize_force_refresh() {
    use worldrunner_render_shadow::RefreshReason;
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let configured = DirectionalShadowSettings {
        refresh_policy: policy(),
        ..settings()
    };
    let mut renderer = ShadowedScene::new(&context, &source, configured).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    let caster = renderer.scene_mut().instance_handles().nth(1).unwrap();
    let (object, _) = renderer.scene_mut().instance(caster).unwrap();
    let mut transform = *object.transform();
    transform.position_millimeters = [6_000, 0, 8_000];
    renderer
        .scene_mut()
        .set_instance_transform(caster, transform)
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(RefreshReason::DepthRange)
    );
    renderer.set_effects(&fixture::effects(-600_000)).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(RefreshReason::Light)
    );
    renderer
        .set_settings(
            &context,
            DirectionalShadowSettings {
                normal_bias_meters: 0.02,
                ..configured
            },
        )
        .unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(RefreshReason::Invalidated)
    );
    let resized = translated_view(
        0,
        crate::Viewport {
            width: 200,
            height: 180,
        },
    );
    let target = OffscreenTarget::new(context.device(), resized.viewport()).unwrap();
    render_target(&mut renderer, &context, &resized, &target).unwrap();
    assert_eq!(renderer.cascade_refresh_states()[0].last_frame(), Some(4));
    assert_eq!(
        renderer.cascade_refresh_states()[0].last_reason(),
        Some(RefreshReason::Invalidated)
    );
}

fn translated_view(shift: i64, viewport: crate::Viewport) -> SceneView {
    SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -4_000 + shift, 6_500],
            target_millimeters: [0, shift, 0],
        },
        *fixture::effects(600_000).camera(),
        viewport,
    )
    .unwrap()
}
