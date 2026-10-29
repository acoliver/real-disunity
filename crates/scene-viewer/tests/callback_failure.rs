use std::{cell::Cell, rc::Rc, time::Duration};

use worldrunner_render_plan::{
    RenderScene, SceneCamera, SceneGeometry, SceneParts, SceneResources,
};
use worldrunner_render_scene::{CameraPose, draw::InstanceError};
use worldrunner_scene_viewer::{OrbitLimits, ViewerError, ViewerOptions, run_with_updates};

struct CallbackDrop(Rc<Cell<bool>>);

impl Drop for CallbackDrop {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

fn start_deadline_watchdog() {
    // An independent watchdog also runs when an occluded event loop is asleep.
    // This executable exits on completion without waiting for the watchdog.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(30));
        eprintln!(
            "callback failure test exceeded its 30-second deadline: \
             window did not produce the required frames (occluded?)"
        );
        std::process::exit(1);
    });
}

// A separate test executable keeps the native event loop on the main thread.
fn main() {
    start_deadline_watchdog();
    let scene = scene();
    let options = options();
    let dropped = Rc::new(Cell::new(false));
    let callback_drop = CallbackDrop(dropped.clone());
    let called = Rc::new(Cell::new(0));
    let callback_called = called.clone();
    let result = run_with_updates(scene, options, move |_| {
        assert!(!callback_drop.0.get());
        let count = callback_called.get() + 1;
        callback_called.set(count);
        if count < 3 {
            return Ok(());
        }
        Err(InstanceError::Resource {
            resource: "callback sentinel",
            index: 17,
        })
    });
    assert!(matches!(
        result,
        Err(ViewerError::Instance(InstanceError::Resource {
            resource: "callback sentinel",
            index: 17,
        }))
    ));
    assert_eq!(called.get(), 3);
    assert!(dropped.get(), "the application must release its callback");
    println!("application callback failure propagates and cleans up: passed");
}

fn options() -> ViewerOptions {
    ViewerOptions {
        camera_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        frame_profile: worldrunner_scene_viewer::FrameProfile::Legacy,
        frames_in_flight: 2,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        adapter_callback: None,
        cadence: worldrunner_scene_viewer::PresentationCadence::default(),
        ssr: None,
        render_clock: worldrunner_scene_viewer::RenderClock::default(),
        title: "Callback failure regression".into(),
        size: [64, 64],
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

fn scene() -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}
