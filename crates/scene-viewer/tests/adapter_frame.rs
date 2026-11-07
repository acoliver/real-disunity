use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use worldrunner_render_gpu::configuration::ConfigurationError;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner_render_plan::{
    RenderScene, SceneCamera, SceneGeometry, SceneParts, SceneResources,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{
    AdapterCallback, FrameProfile, FrameSettings, InitialContent, OrbitLimits, PresentationBudget,
    RetainedDriver, RetainedEvent, RetainedEventHost, RetainedHost, RetainedTick, ToneMap,
    ViewerError, ViewerOptions,
};

fn selected_frame() -> FrameSettings {
    FrameSettings {
        shadows: None,
        frame_profile: FrameProfile::HdrUpscaled {
            samples: 4,
            tone_map: ToneMap::Reinhard,
            scale: RenderScale::new(500_000).unwrap(),
            upscale: UpscaleSettings {
                mode: UpscaleMode::Easu,
                sharpness: None,
            },
        },
        presentation_budget: PresentationBudget(300 * 1024 * 1024).into(),
    }
}

#[derive(Default)]
struct Driver {
    presented: bool,
}
impl RetainedDriver for Driver {
    fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
        eprintln!("adapter frame update {}", tick.frame);
        assert_eq!(tick.frame, 0);
        assert_eq!(host.options().frame_profile, selected_frame().frame_profile);
        assert_eq!(
            host.options().presentation_budget,
            selected_frame().presentation_budget
        );
        Ok(())
    }
    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        eprintln!("adapter frame event {event:?}");
        if let RetainedEvent::Presented { frame } = event {
            assert_eq!(*frame, 0);
            let statistics = host.statistics().unwrap();
            assert_eq!(statistics.presentation_budget_bytes, 300 * 1024 * 1024);
            self.presented = true;
            host.request_exit();
        }
        Ok(())
    }
}

// Both startup paths must run in an Aqua session; the desktop loop owns the main thread.
fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_mins(1));
        eprintln!("adapter frame startup test exceeded its deadline");
        std::process::exit(1);
    });
    let mode = std::env::args().nth(1).unwrap_or_else(|| "desktop".into());
    let calls = Arc::new(AtomicUsize::new(0));
    let reject = mode.ends_with("reject");
    let host = startup_host(&mode, calls.clone());
    let mut driver = Driver::default();
    if mode.ends_with("windowless") {
        let mut run = host.windowless(&mut driver).unwrap();
        assert!(run.frame().unwrap());
        run.shutdown().unwrap();
    } else if reject {
        assert!(matches!(
            host.run(&mut driver),
            Err(ViewerError::Configuration(ConfigurationError::Budget {
                budget: 1,
                ..
            }))
        ));
    } else {
        host.run(&mut driver).unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(driver.presented, !reject);
}

fn startup_host(mode: &str, calls: Arc<AtomicUsize>) -> RetainedHost {
    let reject = mode.ends_with("reject");
    let decide = move |identity: &worldrunner_scene_viewer::AdapterIdentity| {
        eprintln!("adapter frame selected {identity:?}");
        assert!(!identity.name.is_empty());
        calls.fetch_add(1, Ordering::SeqCst);
        let mut frame = selected_frame();
        if reject {
            frame.presentation_budget = PresentationBudget(1).into();
        }
        Ok(Some(frame))
    };
    let mut options = options();
    if mode.starts_with("supplier") {
        RetainedHost::with_initial_content(options, move |identity| {
            Ok(InitialContent {
                scene: scene(),
                settings: decide(identity.adapter)?,
            })
        })
        .unwrap()
    } else {
        options.adapter_callback = Some(AdapterCallback::new(decide));
        RetainedHost::new(scene(), options).unwrap()
    }
}

fn scene() -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn options() -> ViewerOptions {
    ViewerOptions {
        activity_reason: worldrunner_scene_viewer::ActivityReason::default(),
        presentation_budget: PresentationBudget::default().into(),
        capture_pool: worldrunner_scene_viewer::CapturePoolPolicy::default(),
        window_size_unit: worldrunner_scene_viewer::WindowSizeUnit::Physical,
        camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        frame_profile: FrameProfile::Legacy,
        frames_in_flight: 2,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        gpu_timing: false,
        adapter_callback: None,
        cadence: worldrunner_scene_viewer::PresentationCadence::Uncapped,
        ssr: None,
        render_clock: worldrunner_scene_viewer::RenderClock::default(),
        title: "Adapter frame regression".into(),
        size: [800, 600],
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
