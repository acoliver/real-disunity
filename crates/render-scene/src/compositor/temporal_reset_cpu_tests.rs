use super::*;

#[test]
fn temporal_reset_cpu_discards_all_pending_resolves_and_previous_view() {
    let mut history = TemporalHistory {
        jitter: Some(ProjectionJitter::default()),
        frame_index: 7,
        epoch: 3,
        previous: Some(Published {
            view_projection: Mat4::IDENTITY,
            jitter_pixels: [0.25, 0.5],
            extents: [[64, 32], [128, 64]],
            animation_time_ms: 100,
            camera_position: Vec3::X,
        }),
        pending_taa: Some([1; worldrunner_render_post::TAA_UNIFORM_BYTES]),
        pending_temporal: Some(worldrunner_render_post::TemporalBlock {
            render: [64, 32],
            output: [128, 64],
            jitter: [0.25, 0.5],
            phase_count: 32,
            settings: worldrunner_render_post::TaaSettings::default(),
            history_valid: true,
        }),
        pending_camera: Some(worldrunner_render_post::TemporalCamera::NONE),
        ..TemporalHistory::default()
    };
    history.reset();
    assert!(history.previous.is_none());
    assert!(history.pending_temporal.is_none());
    assert!(history.pending_camera.is_none());
    assert_eq!(history.frame_index, 0);
    assert_eq!(history.epoch, 4);
    assert_eq!(history.jitter, Some(ProjectionJitter::default()));
    assert!(
        history.pending_taa.is_none(),
        "the native resolve must not retain its old valid block"
    );
}

#[test]
fn temporal_reset_cpu_preserves_exposure_reset_and_adaptation_clock() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let renderer = FrameRenderer::new(&device);
    for exposure_reset in [false, true] {
        renderer.exposure_reset.set(exposure_reset);
        renderer.post_time.set(2.0);
        renderer.temporal.borrow_mut().frame_index = 7;
        renderer.reset_temporal_history();
        assert_eq!(renderer.projection_jitter_index(), 0);
        assert!(!renderer.has_temporal_history());
        assert_eq!(renderer.exposure_reset.get(), exposure_reset);
        assert_eq!(renderer.post_time.get().to_bits(), 2.0_f32.to_bits());
    }
    renderer.exposure_reset.set(false);
    renderer.reset_exposure();
    assert!(renderer.exposure_reset.get());
}
