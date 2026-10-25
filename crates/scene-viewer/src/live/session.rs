#[cfg(test)]
#[path = "session/startup_tests.rs"]
mod startup_tests;
#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};
use winit::{
    event::{ElementState, Ime, WindowEvent},
    keyboard::Key,
};
use worldrunner_render_scene::Viewport;

use super::{
    LiveCapture, LiveDriver, LiveFrame, LiveInputEvent, LiveOptions, LiveReceipt, LiveTick,
};
use crate::{
    ViewerError,
    app::Content,
    renderer::{Renderer, SceneSource},
};

pub(crate) struct Session<'a> {
    pub(super) driver: &'a mut dyn LiveDriver,
    clock: super::LiveClock,
    options: LiveOptions,
    pub(super) initialized: bool,
    frame: u64,
    elapsed: Duration,
    previous: Option<Instant>,
    pub(super) input: super::ingress::InputBuffer,
    pending: Option<(LiveTick, LiveFrame)>,
    pub(super) startup: super::startup::Startup,
    pub(super) startup_frame: Option<LiveFrame>,
}

impl<'a> Session<'a> {
    pub fn new(driver: &'a mut dyn LiveDriver, mut options: LiveOptions) -> Self {
        options.scheduled_input.sort_by_key(|input| input.frame);
        let schedule = std::mem::take(&mut options.scheduled_input);
        let clock = driver.clock();
        Self {
            driver,
            clock,
            options,
            initialized: false,
            frame: 0,
            elapsed: Duration::ZERO,
            previous: None,
            input: super::ingress::InputBuffer::new(schedule),
            pending: None,
            startup: super::startup::Startup::default(),
            startup_frame: None,
        }
    }

    pub(super) fn start_clock(&mut self, origin: Instant) {
        self.input.start(origin);
        self.previous = self.clock.real_time().then_some(origin);
    }

    fn tick(&mut self, viewport: Viewport) -> Result<LiveTick, ViewerError> {
        self.tick_at(viewport, Instant::now())
    }

    fn tick_at(&mut self, viewport: Viewport, now: Instant) -> Result<LiveTick, ViewerError> {
        let dt = self.options.fixed_dt.unwrap_or_else(|| {
            self.previous
                .map_or(Duration::ZERO, |previous| now.duration_since(previous))
        });
        self.previous = Some(now);
        let samples = self
            .input
            .collect(
                self.frame,
                self.options.fixed_dt.unwrap_or(self.options.frame_interval),
                self.options.input.as_ref(),
            )
            .map_err(ViewerError::Driver)?;
        for sample in &samples {
            self.driver.ingress(sample).map_err(ViewerError::Driver)?;
        }
        Ok(LiveTick {
            frame: self.frame,
            elapsed: self.elapsed,
            dt,
            viewport,
            input: samples.into_iter().map(|sample| sample.event).collect(),
        })
    }

    fn next_frame(&mut self, viewport: Viewport) -> Result<(LiveTick, LiveFrame), ViewerError> {
        if !self.initialized {
            self.driver
                .initialize(viewport)
                .map_err(ViewerError::Driver)?;
            self.start_clock(Instant::now());
            self.initialized = true;
        }
        let tick = self.tick(viewport)?;
        let frame = self.driver.update(&tick).map_err(ViewerError::Driver)?;
        Ok((tick, frame))
    }

    fn prepare(&mut self, renderer: &mut Renderer) -> Result<(), ViewerError> {
        let viewport = renderer.render_viewport();
        let (tick, frame) = self.next_frame(viewport)?;
        super::startup::validate_frame(&frame, viewport)?;
        renderer.prepare_live(&frame)?;
        self.pending = Some((tick, frame));
        Ok(())
    }
}

impl Content for Session<'_> {
    fn title(&self) -> &str {
        &self.options.title
    }
    fn size(&self) -> [u32; 2] {
        self.options.size
    }
    fn interval(&self) -> Duration {
        self.options.frame_interval
    }
    fn source(&self) -> SceneSource<'_> {
        SceneSource::Live
    }
    fn close_immediately(&self) -> bool {
        false
    }

    fn event(&mut self, event: &WindowEvent, _: Option<Viewport>) {
        let acquired = Instant::now();
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = logical_key(&event.logical_key) {
                    self.input.push(
                        LiveInputEvent::Key {
                            key,
                            pressed: event.state == ElementState::Pressed,
                            repeat: event.repeat,
                        },
                        acquired,
                    );
                }
                if event.state == ElementState::Pressed
                    && let Some(text) = &event.text
                {
                    self.input
                        .push(LiveInputEvent::Text(text.to_string()), acquired);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.input
                    .push(LiveInputEvent::Text(text.clone()), acquired);
            }
            WindowEvent::Focused(focused) => {
                self.input.push(LiveInputEvent::Focused(*focused), acquired);
            }
            WindowEvent::Resized(size) => self
                .input
                .push(LiveInputEvent::Resized([size.width, size.height]), acquired),
            WindowEvent::CloseRequested => {
                self.input.push(LiveInputEvent::CloseRequested, acquired);
            }
            _ => {}
        }
    }

    fn redraw(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError> {
        if self.clock == super::LiveClock::ReadyPrewarmed && !self.advance_startup(renderer)? {
            return Ok(false);
        }
        if self.pending.is_none() {
            self.prepare(renderer)?;
        }
        let (tick, frame) = self.pending.as_ref().expect("prepared live contribution");
        let Some(outcome) = renderer.render_capture(&frame.view, frame.capture.is_some())? else {
            return Ok(false);
        };
        let capture = frame
            .capture
            .zip(outcome.capture)
            .map(|(id, image)| LiveCapture { id, image });
        self.driver
            .presented(LiveReceipt {
                frame: tick.frame,
                elapsed: tick.elapsed,
                capture,
            })
            .map_err(ViewerError::Driver)?;
        let exit = frame.exit || tick.input.contains(&LiveInputEvent::CloseRequested);
        self.elapsed = self.elapsed.saturating_add(tick.dt);
        self.frame += 1;
        self.pending = None;
        Ok(exit
            || self
                .options
                .max_frames
                .is_some_and(|limit| self.frame >= limit))
    }
}

fn logical_key(key: &Key) -> Option<String> {
    match key {
        Key::Character(value) if value == " " => Some("space".into()),
        Key::Character(value) => Some(value.to_lowercase()),
        Key::Named(value) => Some(format!("{value:?}").to_lowercase()),
        _ => None,
    }
}
