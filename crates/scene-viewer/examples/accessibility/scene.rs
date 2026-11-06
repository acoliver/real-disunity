use worldrunner_render_plan::{
    RenderScene, SceneCamera, SceneGeometry, SceneParts, SceneResources, SceneValidationError,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{FrameProfile, OrbitLimits, ViewerOptions};

pub fn scene() -> Result<RenderScene, SceneValidationError> {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(Vec::new(), Vec::new())?,
        geometry: SceneGeometry::new(Vec::new(), Vec::new())?,
        terrain: None,
        effects: None,
    })
}

pub fn options() -> ViewerOptions {
    ViewerOptions {
        activity_reason: worldrunner_scene_viewer::ActivityReason::default(),
        presentation_budget: worldrunner_scene_viewer::PresentationBudget::default().into(),
        capture_pool: worldrunner_scene_viewer::CapturePoolPolicy::default(),
        window_size_unit: worldrunner_scene_viewer::WindowSizeUnit::default(),
        camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        title: "Native accessibility: Tab, Enter, arrows, Escape".into(),
        size: [800, 600],
        pose: CameraPose {
            eye_millimeters: [0, -5000, 2500],
            target_millimeters: [0, 0, 800],
        },
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        orbit: OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 100_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
        shadows: None,
        frame_profile: FrameProfile::Legacy,
        frames_in_flight: 2,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        gpu_timing: false,
        adapter_callback: None,
        cadence: worldrunner_scene_viewer::PresentationCadence::default(),
        ssr: None,
        render_clock: worldrunner_scene_viewer::RenderClock::default(),
    }
}
