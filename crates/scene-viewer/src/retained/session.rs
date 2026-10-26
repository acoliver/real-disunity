#[cfg(test)]
mod tests;

use crate::{
    RetainedDriver, RetainedEvent, RetainedHost, RetainedInput, RetainedTick, ViewerError,
    app::Content,
    renderer::{Renderer, SceneSource},
};
use std::time::Instant;
use winit::{
    event::{ElementState, WindowEvent},
    keyboard::{Key, PhysicalKey},
};
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
    fn title(&self) -> &str {
        &self.host.options.title
    }
    fn size(&self) -> [u32; 2] {
        self.host.options.size
    }
    fn source(&self) -> SceneSource<'_> {
        SceneSource::Static {
            scene: &self.host.scene,
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
        match event {
            WindowEvent::Resized(size) => {
                self.input(RetainedInput::Resized([size.width, size.height]));
            }
            WindowEvent::Focused(focus) => self.input(RetainedInput::Focused(*focus)),
            WindowEvent::Occluded(value) => self.input(RetainedInput::Occluded(*value)),
            WindowEvent::CloseRequested => self.input(RetainedInput::CloseRequested),
            WindowEvent::KeyboardInput { event, .. } => {
                let key = match &event.logical_key {
                    Key::Character(value) => value.to_lowercase(),
                    Key::Named(value) => format!("{value:?}").to_lowercase(),
                    _ => match event.physical_key {
                        PhysicalKey::Code(code) => format!("{code:?}").to_lowercase(),
                        PhysicalKey::Unidentified(_) => return,
                    },
                };
                self.input(RetainedInput::Key {
                    key,
                    pressed: event.state == ElementState::Pressed,
                    repeat: event.repeat,
                });
            }
            _ => {}
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
        if renderer.take_surface_recreated() {
            self.input(RetainedInput::SurfaceRecreated);
        }
        Ok(self.host.exit)
    }
}
