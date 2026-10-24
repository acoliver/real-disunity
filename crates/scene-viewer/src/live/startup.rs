//! `ReadyPrewarmed` startup: present the initialized scene before the clock origin.

use std::time::Instant;
use worldrunner_render_scene::Viewport;

use super::{LiveFrame, LiveInputEvent, session::Session};
use crate::{ViewerError, renderer::Renderer};

/// Startup progress. Only `Running` admits gameplay updates.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) enum Startup {
    /// Initialize and present the initial scene on the next redraw.
    #[default]
    Pending,
    /// The initial scene completed on the GPU at this extent. The next redraw,
    /// after the event loop has delivered queued events, commits the origin.
    Presented(Viewport),
    Running,
}

pub(super) fn validate_frame(frame: &LiveFrame, viewport: Viewport) -> Result<(), ViewerError> {
    if frame.view.viewport() != viewport {
        return Err(ViewerError::InvalidOptions(
            "live frame view must match tick viewport",
        ));
    }
    if frame
        .clear
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(ViewerError::InvalidOptions(
            "live clear channels must be finite and in 0..=1",
        ));
    }
    Ok(())
}

fn rejected(message: &str) -> ViewerError {
    ViewerError::Driver(format!("ready_prewarmed_v1 startup rejected: {message}"))
}

impl Session<'_> {
    /// Advance startup by one redraw. Returns true once gameplay updates may run.
    pub(super) fn advance_startup(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError> {
        match self.startup {
            Startup::Running => Ok(true),
            Startup::Pending => {
                self.present_initial(renderer)?;
                Ok(false)
            }
            Startup::Presented(ready) => {
                self.commit_origin(ready, renderer.render_viewport(), Instant::now())?;
                Ok(true)
            }
        }
    }

    fn present_initial(&mut self, renderer: &mut Renderer) -> Result<(), ViewerError> {
        let viewport = renderer.render_viewport();
        if self.startup_frame.is_none() {
            let frame = self.initial_frame(viewport)?;
            renderer.prepare_live(&frame)?;
            self.startup_frame = Some(frame);
        }
        let frame = self.startup_frame.as_ref().expect("startup frame");
        if frame.view.viewport() != viewport {
            return Err(rejected(
                "surface extent changed before the initial scene presented",
            ));
        }
        // Skip/Reconfigure/Recreate keep the retained frame; no second initialization occurs.
        if renderer.render_capture(&frame.view, false)?.is_some() {
            renderer.wait_idle()?;
            self.startup = Startup::Presented(viewport);
        }
        Ok(())
    }

    pub(super) fn initial_frame(&mut self, viewport: Viewport) -> Result<LiveFrame, ViewerError> {
        if !self.initialized {
            self.driver
                .initialize(viewport)
                .map_err(ViewerError::Driver)?;
            self.initialized = true;
        }
        let frame = self
            .driver
            .prepare_initial(viewport)
            .map_err(ViewerError::Driver)?
            .ok_or_else(|| rejected("driver supplied no initial scene"))?;
        validate_frame(&frame, viewport)?;
        if frame.capture.is_some() || frame.exit {
            return Err(rejected("initial scene cannot capture or exit"));
        }
        Ok(frame)
    }

    /// Set the single origin, after verifying no gameplay input preceded it.
    pub(super) fn commit_origin(
        &mut self,
        ready: Viewport,
        current: Viewport,
        now: Instant,
    ) -> Result<(), ViewerError> {
        if ready != current {
            return Err(rejected(
                "surface extent changed after the initial scene presented",
            ));
        }
        if self.input.any_queued(|event| {
            matches!(event, LiveInputEvent::Key { .. } | LiveInputEvent::Text(_))
        }) {
            return Err(rejected(
                "key or text input was acquired before the ready origin; release and press after ready",
            ));
        }
        self.start_clock(now);
        self.startup = Startup::Running;
        self.startup_frame = None;
        self.driver.ready().map_err(ViewerError::Driver)
    }
}
