use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::Viewport;

use crate::{
    LiveDriver, LiveOptions, ViewerError, ViewerOptions,
    renderer::{Renderer, SceneSource},
};

mod static_scene;

pub(super) trait Content {
    fn title(&self) -> &str;
    fn size(&self) -> [u32; 2];
    fn interval(&self) -> Duration {
        Duration::from_millis(16)
    }
    fn source(&self) -> SceneSource<'_>;
    fn event(&mut self, event: &WindowEvent, viewport: Option<Viewport>);
    fn close_immediately(&self) -> bool {
        true
    }
    fn exit_requested(&self) -> bool {
        false
    }
    /// Returns true after the final frame has been presented.
    fn redraw(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError>;
}

pub(super) fn run(scene: RenderScene, options: ViewerOptions) -> Result<(), ViewerError> {
    run_content(Box::new(static_scene::StaticScene::new(scene, options)?))
}

pub(super) fn run_live(
    driver: &mut dyn LiveDriver,
    options: LiveOptions,
) -> Result<(), ViewerError> {
    run_content(Box::new(crate::live::Session::new(driver, options)))
}

fn run_content(content: Box<dyn Content + '_>) -> Result<(), ViewerError> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(content);
    let result = event_loop.run_app(&mut app);
    app.renderer = None;
    if let Some(error) = app.failure {
        return Err(error);
    }
    result?;
    Ok(())
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
        self.next_frame = Instant::now() + self.content.interval();
        Ok(())
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = false;
        let result = if let Some(renderer) = &mut self.renderer {
            let size = renderer.window.inner_size();
            renderer.resize([size.width, size.height])
        } else {
            let size = self.content.size();
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(self.content.title())
                        .with_inner_size(winit::dpi::LogicalSize::new(size[0], size[1])),
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
            renderer.window.request_redraw();
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = true;
        self.content.event(&WindowEvent::Focused(false), None);
        if let Some(renderer) = &mut self.renderer
            && let Err(error) = renderer.resize([0, 0])
        {
            self.fail(event_loop, error);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        if renderer.window.id() != id {
            return;
        }
        self.content.event(&event, renderer.viewport);
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
                    renderer.window.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if let Err(error) = renderer.resize([size.width, size.height]) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Occluded(value) => self.occluded = value,
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        if let Err(error) = renderer.check_errors() {
            self.fail(event_loop, error);
            return;
        }
        if self.occluded || self.suspended || renderer.viewport.is_none() {
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            if Instant::now() >= self.next_frame {
                renderer.window.request_redraw();
                self.next_frame = Instant::now() + self.content.interval();
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }
}
