use std::{cell::Cell, rc::Rc};

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

// A separate test executable keeps the native event loop on the main thread.
fn main() {
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap();
    let options = ViewerOptions {
        frame_profile: worldrunner_scene_viewer::FrameProfile::Legacy,
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
    };
    let dropped = Rc::new(Cell::new(false));
    let callback_drop = CallbackDrop(dropped.clone());
    let called = Rc::new(Cell::new(false));
    let callback_called = called.clone();
    let result = run_with_updates(scene, options, move |_| {
        assert!(!callback_drop.0.get());
        callback_called.set(true);
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
    assert!(called.get());
    assert!(dropped.get(), "the application must release its callback");
    println!("application callback failure propagates and cleans up: passed");
}
