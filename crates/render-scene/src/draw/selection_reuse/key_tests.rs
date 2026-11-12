use super::*;
use crate::view::{CameraPose, SceneView, Viewport};
use worldrunner_render_plan::SceneCamera;

#[test]
fn fade_eye_bits_invalidate_camera_caster_and_compaction_selection_keys() {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -4000, 1000],
            target_millimeters: [0, 0, 1000],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 1,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: 64,
            height: 64,
        },
    )
    .unwrap();
    let frame = FrameUniform::new(&view, None).unwrap();
    for caster in [false, true] {
        let original = Key::new(0, frame.view_projection, &frame, caster);
        let mut changed = frame;
        changed.camera_position.y = changed.camera_position.y.next_up();
        assert_eq!(
            changed.camera_position_millimeters,
            frame.camera_position_millimeters
        );
        assert_ne!(
            Key::new(0, frame.view_projection, &changed, caster),
            original
        );
        assert_eq!(Key::new(0, frame.view_projection, &frame, caster), original);
        changed = frame;
        changed.camera_position_millimeters[1] += 1;
        assert_ne!(
            Key::new(0, frame.view_projection, &changed, caster),
            original
        );
    }
}
