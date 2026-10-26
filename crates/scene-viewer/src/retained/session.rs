#[cfg(test)]
pub(crate) mod pointer_tests;
#[cfg(test)]
mod tests;

use crate::{
    RetainedDriver, RetainedEvent, RetainedHost, RetainedInput, RetainedTick, ViewerError,
    app::Content,
    renderer::{Renderer, SceneSource},
};
use std::time::Instant;
use winit::event::{DeviceEvent, WindowEvent};
use worldrunner_render_scene::Viewport;

pub(crate) struct Session<'a> {
    host: RetainedHost,
    driver: &'a mut dyn RetainedDriver,
    start: Instant,
    previous: Instant,
    frame: u64,
    pending: bool,
    failure: Option<String>,
}

impl<'a> Session<'a> {
    pub fn new(host: RetainedHost, driver: &'a mut dyn RetainedDriver) -> Self {
        let start = Instant::now();
        Self {
            host,
            driver,
            start,
            previous: start,
            frame: 0,
            pending: false,
            failure: None,
        }
    }

    fn input(&mut self, input: RetainedInput) {
        for input in self.host.input(input) {
            if let Err(error) = self.driver.event(&RetainedEvent::Input(input)) {
                self.failure = Some(error);
                break;
            }
        }
    }

    fn notifications(&mut self) -> Result<(), ViewerError> {
        while let Some(event) = self.host.captures.take_event() {
            self.driver.event(&event).map_err(ViewerError::Driver)?;
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), ViewerError> {
        let now = Instant::now();
        let tick = RetainedTick {
            frame: self.frame,
            elapsed: now - self.start,
            dt: now - self.previous,
        };
        self.driver
            .update(&mut self.host, &tick)
            .map_err(ViewerError::Driver)?;
        self.previous = now;
        self.pending = true;
        Ok(())
    }
}

impl Content for Session<'_> {
    fn poll(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError> {
        let pending = renderer.collect_retained(&mut self.host)?;
        self.notifications()?;
        Ok(pending)
    }
    fn shutdown(&mut self) -> Result<(), ViewerError> {
        self.host.exit = true;
        self.host.captures.cancel();
        self.notifications()
    }
    fn title(&self) -> &str {
        &self.host.options.title
    }
    fn size(&self) -> [u32; 2] {
        self.host.options.size
    }
    fn cadence(&self) -> crate::PresentationCadence {
        self.host.options.cadence
    }
    fn window_ready(&mut self, window: std::sync::Arc<winit::window::Window>) {
        self.host.input_state.window = Some(window);
    }
    fn device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.input(RetainedInput::RawMotion([delta.0, delta.1]));
        }
    }
    fn source(&self) -> SceneSource<'_> {
        SceneSource::Static {
            scene: &self.host.initial_scene,
            shadows: self.host.options.shadows,
            profile: self.host.options.frame_profile,
            frames_in_flight: self.host.options.frames_in_flight,
        }
    }
    fn exit_requested(&self) -> bool {
        self.host.exit
    }
    fn take_failure(&mut self) -> Option<ViewerError> {
        self.failure.take().map(ViewerError::Driver)
    }
    fn surface_size(&mut self, size: [u32; 2]) {
        self.input(RetainedInput::Resized(size));
    }
    fn event(&mut self, event: &WindowEvent, _viewport: Option<Viewport>) {
        if let Some(input) = super::input::window_input(event) {
            self.input(input);
        }
    }
    fn redraw(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError> {
        if let Some(error) = self.failure.take() {
            return Err(ViewerError::Driver(error));
        }
        if !self.pending {
            self.tick()?;
        }
        if renderer.render_retained(&mut self.host)? {
            self.driver
                .event(&RetainedEvent::Presented { frame: self.frame })
                .map_err(ViewerError::Driver)?;
            self.frame += 1;
            self.pending = false;
        }
        self.notifications()?;
        if renderer.take_surface_recreated() {
            self.input(RetainedInput::SurfaceRecreated);
        }
        Ok(self.host.exit)
    }
}
