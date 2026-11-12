//! When Play stops showing the window's opening view and starts at the game's camera.
use worldrunner_camera::CameraState;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_scene::CameraPose;

use crate::scene_transform::scene_camera;

/// Tracks whether the window has started at the game's camera, and the last reason it could
/// not, so a frame-rate loop reports each reason once.
#[derive(Debug, Default)]
pub(crate) struct CameraStart {
    started: bool,
    reported: Option<String>,
}

/// What one frame decides about starting at the game's camera.
#[derive(Debug, PartialEq)]
pub(crate) struct StartStep {
    /// The pose and lens to start the window at, on the frame that starts it.
    pub start: Option<(CameraPose, SceneCamera)>,
    /// Why the window keeps its view, when the reason differs from the last one reported.
    pub note: Option<String>,
}

impl CameraStart {
    /// Decides one frame. Before any frame shows objects, and after the window has started at
    /// the game's camera, nothing changes: movement after the start belongs to navigation. A frame
    /// that shows objects starts at the game's camera when the game presents one the viewer can
    /// draw, and otherwise is tried again on the next frame that shows objects.
    pub(crate) fn next(&mut self, shows_objects: bool, game: Option<&CameraState>) -> StartStep {
        let quiet = StartStep {
            start: None,
            note: None,
        };
        if self.started || !shows_objects {
            return quiet;
        }
        let reason = match game.map(scene_camera) {
            Some(Ok(start)) => {
                self.started = true;
                return StartStep {
                    start: Some(start),
                    note: None,
                };
            }
            Some(Err(error)) => format!("the game's camera is not shown: {error}"),
            None => "the game presents no camera; the window keeps its default view".to_owned(),
        };
        if self.reported.as_ref() == Some(&reason) {
            return quiet;
        }
        self.reported = Some(reason.clone());
        StartStep {
            start: None,
            note: Some(reason),
        }
    }
}

#[cfg(test)]
#[path = "camera_start_tests.rs"]
mod tests;
