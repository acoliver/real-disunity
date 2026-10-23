use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};
use worldrunner_render_plan::RenderScene;

use crate::{OrbitCamera, ViewerError, ViewerOptions, renderer::Renderer};

pub(super) fn run(
    scene: RenderScene,
    options: ViewerOptions,
    update: crate::SceneUpdate,
) -> Result<(), ViewerError> {
    let camera = OrbitCamera::new(options.pose, options.orbit)?;
    let event_loop = EventLoop::new()?;
    let mut app = App {
        update,
        scene,
        options,
        camera,
        renderer: None,
        failure: None,
        dragging: false,
        cursor: None,
        occluded: false,
        suspended: false,
        next_frame: Instant::now(),
    };
    let result = event_loop.run_app(&mut app);
    // Drop the surface/window/GPU before returning, including on callback failure.
    cleanup(&mut app.renderer);
    if let Some(error) = app.failure {
        return Err(error);
    }
    result?;
    Ok(())
}

pub(super) fn cleanup(renderer: &mut Option<Renderer>) {
    *renderer = None;
}

struct App {
    update: crate::SceneUpdate,
    scene: RenderScene,
    options: ViewerOptions,
    camera: OrbitCamera,
    renderer: Option<Renderer>,
    failure: Option<ViewerError>,
    dragging: bool,
    cursor: Option<[f64; 2]>,
    occluded: bool,
    suspended: bool,
    next_frame: Instant,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: ViewerError) {
        self.failure = Some(error);
        event_loop.exit();
    }

    fn handle_key(&mut self, event_loop: &ActiveEventLoop, key: Key) {
        match key {
            Key::Named(NamedKey::Escape) => event_loop.exit(),
            Key::Named(NamedKey::Home) => self.camera.reset(),
            Key::Character(key) if key.eq_ignore_ascii_case("r") => self.camera.reset(),
            _ => {}
        }
    }

    fn set_occluded(&mut self, value: bool) {
        self.occluded = value;
        if !value && let Some(renderer) = &self.renderer {
            renderer.window.request_redraw();
        }
        self.clear_drag();
    }

    fn clear_drag(&mut self) {
        self.dragging = false;
        self.cursor = None;
    }

    fn redraw(&mut self) -> Result<(), ViewerError> {
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };
        if self.occluded || self.suspended {
            return Ok(());
        }
        if let Some(viewport) = renderer.viewport {
            let view = self.camera.view(self.options.camera, viewport)?;
            renderer.render(&view, &mut self.update)?;
        }
        self.next_frame = Instant::now() + Duration::from_millis(16);
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = false;
        if let Some(renderer) = &mut self.renderer {
            let size = renderer.window.inner_size();
            if let Err(error) = renderer.resize([size.width, size.height]) {
                self.fail(event_loop, error);
                return;
            }
            renderer.window.request_redraw();
            return;
        }
        let result = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(&self.options.title)
                    .with_inner_size(winit::dpi::LogicalSize::new(
                        self.options.size[0],
                        self.options.size[1],
                    )),
            )
            .map_err(ViewerError::from)
            .and_then(|window| {
                pollster::block_on(Renderer::new(
                    event_loop.owned_display_handle(),
                    Arc::new(window),
                    &self.scene,
                    self.options.shadows,
                    self.options.frame_profile,
                    self.options.frames_in_flight,
                ))
            });
        match result {
            Ok(renderer) => {
                renderer.window.request_redraw();
                self.renderer = Some(renderer);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.suspended = true;
        self.clear_drag();
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
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(error) = renderer.resize([size.width, size.height]) {
                    self.fail(event_loop, error);
                    return;
                }
                self.clear_drag();
            }
            WindowEvent::Occluded(value) => self.set_occluded(value),
            WindowEvent::Focused(false) | WindowEvent::CursorLeft { .. } => self.clear_drag(),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
                if !self.dragging {
                    self.cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let current = [position.x, position.y];
                if self.dragging
                    && let (Some(previous), Some(viewport)) = (self.cursor, renderer.viewport)
                {
                    self.camera.drag(
                        [current[0] - previous[0], current[1] - previous[1]],
                        viewport,
                    );
                }
                self.cursor = Some(current);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => f64::from(y),
                    MouseScrollDelta::PixelDelta(position) => position.y / 40.0,
                };
                self.camera.zoom(steps);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.handle_key(event_loop, event.logical_key);
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
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
        if let Err(error) = renderer.poll() {
            self.fail(event_loop, error);
            return;
        }
        if self.occluded || self.suspended || renderer.viewport.is_none() {
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            if Instant::now() >= self.next_frame {
                renderer.window.request_redraw();
                self.next_frame = Instant::now() + Duration::from_millis(16);
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
    }
}
