use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::Viewport;

use crate::{
    LiveDriver, LiveOptions, PresentationCadence, ViewerError, ViewerOptions,
    renderer::{Renderer, SceneSource},
};

mod cadence;
mod static_scene;

pub(super) trait Content {
    fn title(&self) -> &str;
    fn size(&self) -> [u32; 2];
    fn window_size_unit(&self) -> crate::WindowSizeUnit {
        crate::WindowSizeUnit::default()
    }
    fn interval(&self) -> Duration {
        Duration::from_millis(16)
    }
    fn cadence(&self) -> PresentationCadence {
        PresentationCadence::Fixed(self.interval())
    }
    fn window_ready(&mut self, _window: Arc<Window>) {}
    fn device_event(&mut self, _event: &DeviceEvent) {}
    fn source(&self) -> SceneSource<'_>;
    fn event(&mut self, event: &WindowEvent, viewport: Option<Viewport>);
    fn surface_size(&mut self, _size: [u32; 2]) {}
    fn close_immediately(&self) -> bool {
        true
    }
    fn exit_requested(&self) -> bool {
        false
    }
    fn take_failure(&mut self) -> Option<ViewerError> {
        None
    }
    fn shutdown(&mut self) -> Result<(), ViewerError> {
        Ok(())
    }
    /// Progresses submitted work without simulation updates or presentation.
    /// Returns whether completion polling is still needed.
    fn poll(&mut self, _renderer: &mut Renderer) -> Result<bool, ViewerError> {
        Ok(false)
    }
    /// Returns true after the final frame has been presented.
    fn redraw(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError>;
}

pub(super) fn run(
    scene: RenderScene,
    options: ViewerOptions,
    update: crate::SceneUpdate,
) -> Result<(), ViewerError> {
    run_content(Box::new(static_scene::StaticScene::new(
        scene, options, update,
    )?))
}

pub(super) fn run_live(
    driver: &mut dyn LiveDriver,
    options: LiveOptions,
) -> Result<(), ViewerError> {
    run_content(Box::new(crate::live::Session::new(driver, options)))
}
pub(super) fn run_retained(
    host: crate::RetainedHost,
    driver: &mut dyn crate::RetainedDriver,
) -> Result<(), ViewerError> {
    run_content(Box::new(crate::retained::Session::new(host, driver)))
}

fn run_content(content: Box<dyn Content + '_>) -> Result<(), ViewerError> {
    let mut app = App::new(content);
    let result = EventLoop::new().and_then(|event_loop| event_loop.run_app(&mut app));
    let shutdown = app.content.shutdown();
    let drain = app.renderer.as_ref().map(Renderer::drain).transpose();
    cleanup(&mut app.renderer);
    if let Some(error) = app.failure {
        return Err(error);
    }
    shutdown?;
    drain?;
    result?;
    Ok(())
}

pub(super) fn cleanup(renderer: &mut Option<Renderer>) {
    *renderer = None;
}

struct App<'a> {
    content: Box<dyn Content + 'a>,
    renderer: Option<Renderer>,
    failure: Option<ViewerError>,
    occluded: bool,
    suspended: bool,
    next_frame: Instant,
}

impl<'a> App<'a> {
    fn new(content: Box<dyn Content + 'a>) -> Self {
        Self {
            content,
            renderer: None,
            failure: None,
            occluded: false,
            suspended: false,
            next_frame: Instant::now(),
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: ViewerError) {
        self.failure = Some(error);
        event_loop.exit();
    }

    fn event_actions(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(error) = self.content.take_failure() {
            self.fail(event_loop, error);
        } else if self.content.exit_requested() {
            event_loop.exit();
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), ViewerError> {
        if self.occluded || self.suspended {
            return Ok(());
        }
        if let Some(renderer) = &mut self.renderer
            && renderer.viewport.is_some()
            && self.content.redraw(renderer)?
        {
            event_loop.exit();
        }
        self.next_frame = cadence::next_frame(Instant::now(), self.content.cadence());
        Ok(())
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = false;
        let result = if let Some(renderer) = &mut self.renderer {
            let size = renderer
                .window
                .as_ref()
                .expect("windowed renderer")
                .inner_size();
            renderer.resize([size.width, size.height])
        } else {
            let size = self.content.size();
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(self.content.title())
                        .with_inner_size(self.content.window_size_unit().creation_size(size)),
                )
                .map_err(ViewerError::from)
                .and_then(|window| {
                    pollster::block_on(Renderer::from_source(
                        event_loop.owned_display_handle(),
                        Arc::new(window),
                        self.content.source(),
                    ))
                    .map(|renderer| self.renderer = Some(renderer))
                })
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
            return;
        }
        if let Some(renderer) = &self.renderer {
            let size = renderer
                .window
                .as_ref()
                .expect("windowed renderer")
                .inner_size();
            self.content
                .window_ready(renderer.window.as_ref().expect("windowed renderer").clone());
            self.content.surface_size([size.width, size.height]);
            renderer
                .window
                .as_ref()
                .expect("windowed renderer")
                .request_redraw();
        }
        self.event_actions(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = true;
        self.content.event(&WindowEvent::Focused(false), None);
        self.content.surface_size([0, 0]);
        if let Some(renderer) = &mut self.renderer
            && let Err(error) = renderer.resize([0, 0])
        {
            self.fail(event_loop, error);
            return;
        }
        self.event_actions(event_loop);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        self.content.device_event(&event);
        self.event_actions(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        if renderer.window.as_ref().expect("windowed renderer").id() != id {
            return;
        }
        self.content.event(&event, renderer.viewport);
        if let Some(error) = self.content.take_failure() {
            self.fail(event_loop, error);
            return;
        }
        if self.content.exit_requested() {
            event_loop.exit();
            return;
        }
        match event {
            WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::CloseRequested => {
                if self.content.close_immediately()
                    || self.occluded
                    || self.suspended
                    || renderer.viewport.is_none()
                {
                    event_loop.exit();
                } else {
                    renderer
                        .window
                        .as_ref()
                        .expect("windowed renderer")
                        .request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if let Err(error) = renderer.resize([size.width, size.height]) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Occluded(value) => {
                self.occluded = value;
                if !value {
                    renderer
                        .window
                        .as_ref()
                        .expect("windowed renderer")
                        .request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(error) = self.content.take_failure() {
            self.fail(event_loop, error);
            return;
        }
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        let pending = match renderer.poll().and_then(|()| self.content.poll(renderer)) {
            Ok(pending) => pending,
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };
        if self.content.exit_requested() {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        let active = !self.occluded && !self.suspended && renderer.viewport.is_some();
        let (redraw, flow) = cadence::schedule(
            self.content.cadence(),
            now,
            self.next_frame,
            active,
            pending,
        );
        if redraw {
            renderer
                .window
                .as_ref()
                .expect("windowed renderer")
                .request_redraw();
            self.next_frame = cadence::next_frame(now, self.content.cadence());
        }
        event_loop.set_control_flow(flow);
    }
}
