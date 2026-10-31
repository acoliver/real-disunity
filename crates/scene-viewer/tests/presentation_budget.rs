use std::time::Duration;
use worldrunner_render_gpu::configuration::ConfigurationError;
use worldrunner_render_plan::{
    RenderScene, SceneCamera, SceneGeometry, SceneParts, SceneResources,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{
    FrameProfile, OrbitLimits, PresentationBudget, PresentationCadence, RenderClock,
    RetainedDriver, RetainedHost, RetainedTick, ViewerError, ViewerOptions,
};

struct Driver(bool);
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        self.0 = true;
        Err("presentation budget startup reached".into())
    }
}

// Native event-loop construction must run on the executable's main thread.
fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(30));
        eprintln!("presentation budget desktop test exceeded its deadline");
        std::process::exit(1);
    });
    let reject = std::env::args().any(|arg| arg == "--reject");
    if !reject {
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--reject")
                .status()
                .unwrap()
                .success()
        );
    }
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let mut driver = Driver(false);
    let mut options = options();
    options.presentation_budget =
        PresentationBudget(if reject { 256 } else { 300 } * 1024 * 1024).into();
    let result = RetainedHost::new(scene, options).unwrap().run(&mut driver);
    if reject {
        assert!(!driver.0);
        assert!(matches!(result,
            Err(ViewerError::Configuration(ConfigurationError::Budget { required, budget }))
            if required > budget && budget == 256 * 1024 * 1024));
    } else {
        assert!(driver.0);
        assert!(matches!(result, Err(ViewerError::Driver(message))
            if message == "presentation budget startup reached"));
    }
}

fn options() -> ViewerOptions {
    ViewerOptions {
        presentation_budget: PresentationBudget::default().into(),
        capture_pool: worldrunner_scene_viewer::CapturePoolPolicy::default(),
        window_size_unit: worldrunner_scene_viewer::WindowSizeUnit::default(),
        camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        frame_profile: FrameProfile::Legacy,
        frames_in_flight: 2,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        gpu_timing: false,
        adapter_callback: None,
        cadence: PresentationCadence::default(),
        ssr: None,
        render_clock: RenderClock::default(),
        title: "Presentation budget regression".into(),
        size: [8, 8],
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        pose: CameraPose {
            eye_millimeters: [0, -2000, 2000],
            target_millimeters: [0; 3],
        },
        orbit: OrbitLimits {
            min_distance_millimeters: 100.0,
            max_distance_millimeters: 10_000.0,
            min_elevation_radians: -1.4,
            max_elevation_radians: 1.4,
        },
        shadows: None,
    }
}
