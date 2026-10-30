use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key, NamedKey},
};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::Viewport;

use super::Content;
use crate::{
    OrbitCamera, ViewerError, ViewerOptions,
    renderer::{Renderer, SceneSource},
};

pub(super) struct StaticScene {
    scene: RenderScene,
    update: crate::SceneUpdate,
    options: ViewerOptions,
    camera: OrbitCamera,
    dragging: bool,
    cursor: Option<[f64; 2]>,
    exit: bool,
}

impl StaticScene {
    pub fn new(
        scene: RenderScene,
        options: ViewerOptions,
        update: crate::SceneUpdate,
    ) -> Result<Self, ViewerError> {
        let camera = OrbitCamera::new(options.pose, options.orbit)?;
        Ok(Self {
            scene,
            update,
            options,
            camera,
            dragging: false,
            cursor: None,
            exit: false,
        })
    }

    fn clear_drag(&mut self) {
        self.dragging = false;
        self.cursor = None;
    }

    fn key(&mut self, key: &Key) {
        match key {
            Key::Named(NamedKey::Escape) => self.exit = true,
            Key::Named(NamedKey::Home) => self.camera.reset(),
            Key::Character(key) if key.eq_ignore_ascii_case("r") => self.camera.reset(),
            _ => {}
        }
    }
}

impl Content for StaticScene {
    fn title(&self) -> &str {
        &self.options.title
    }
    fn window_size_unit(&self) -> crate::WindowSizeUnit {
        self.options.window_size_unit
    }
    fn size(&self) -> [u32; 2] {
        self.options.size
    }
    fn exit_requested(&self) -> bool {
        self.exit
    }
    fn cadence(&self) -> crate::PresentationCadence {
        self.options.cadence
    }
    fn source(&self) -> SceneSource<'_> {
        SceneSource::Static {
            scene: &self.scene,
            shadows: self.options.shadows,
            profile: self.options.frame_profile,
            startup: (&self.options).into(),
        }
    }

    fn event(&mut self, event: &WindowEvent, viewport: Option<Viewport>) {
        match event {
            WindowEvent::Resized(_)
            | WindowEvent::Occluded(_)
            | WindowEvent::Focused(false)
            | WindowEvent::CursorLeft { .. } => self.clear_drag(),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dragging = *state == ElementState::Pressed;
                if !self.dragging {
                    self.cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let current = [position.x, position.y];
                if self.dragging
                    && let (Some(previous), Some(viewport)) = (self.cursor, viewport)
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
                    MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
                    MouseScrollDelta::PixelDelta(position) => position.y / 40.0,
                };
                self.camera.zoom(steps);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.key(&event.logical_key);
            }
            _ => {}
        }
    }

    fn redraw(&mut self, renderer: &mut Renderer) -> Result<bool, ViewerError> {
        if let Some(viewport) = renderer.viewport {
            let view = worldrunner_render_scene::SceneView::with_convention(
                self.camera.pose(),
                self.options.camera,
                viewport,
                self.options.camera_convention,
            )?;
            renderer.set_clock(self.options.render_clock);
            renderer.render(&view, &mut self.update)?;
        }
        Ok(self.exit)
    }
}
