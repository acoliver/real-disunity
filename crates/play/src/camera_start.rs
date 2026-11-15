//! How Play's window follows the game's camera.
use worldrunner_camera::CameraState;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_scene::CameraPose;

use crate::scene_transform::scene_camera;

/// Follows the game's camera: the window shows the pose and lens the game presents on every frame
/// that shows objects, and reports each reason it cannot, once, so a frame-rate loop does not repeat
/// it. Until the game has presented a usable camera the window keeps its opening view, which
/// navigation may move; after that the game owns the view, as it does in its own player.
#[derive(Debug, Default)]
pub(crate) struct CameraStart {
    shown: Option<(CameraPose, SceneCamera)>,
    reported: Option<String>,
}

/// What one frame decides about the game's camera.
#[derive(Debug, PartialEq)]
pub(crate) struct StartStep {
    /// The pose and lens to show, on a frame where the game's camera differs from the one shown.
    pub start: Option<(CameraPose, SceneCamera)>,
    /// Why the window does not show the game's camera, when the reason differs from the last one
    /// reported.
    pub note: Option<String>,
}

impl CameraStart {
    /// Whether the window has shown a game camera, after which the game owns the view.
    pub(crate) fn following(&self) -> bool {
        self.shown.is_some()
    }

    /// Decides one frame. A frame that shows no objects changes nothing. A frame that shows objects
    /// shows the game's camera when the game presents one the viewer can draw and it differs from
    /// the one shown; an unchanged camera sends nothing. A missing or refused camera keeps the view
    /// the window has and is tried again on the next frame.
    pub(crate) fn next(&mut self, shows_objects: bool, game: Option<&CameraState>) -> StartStep {
        let quiet = StartStep {
            start: None,
            note: None,
        };
        if !shows_objects {
            return quiet;
        }
        let reason = match game.map(scene_camera) {
            Some(Ok(view)) => {
                self.reported = None;
                if self.shown.as_ref() == Some(&view) {
                    return quiet;
                }
                self.shown = Some(view);
                return StartStep {
                    start: Some(view),
                    note: None,
                };
            }
            Some(Err(error)) => format!("the game's camera is not shown: {error}"),
            None if self.following() => {
                "the game presents no camera; the window keeps the last one shown".to_owned()
            }
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
