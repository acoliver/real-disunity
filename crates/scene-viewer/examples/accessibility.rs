//! Opt-in native accessibility check. Pass a TrueType glyf font path explicitly.
//! `cargo run -p worldrunner-scene-viewer --example accessibility -- /path/font.ttf`
#[path = "accessibility/menu.rs"]
mod menu;
#[path = "accessibility/scene.rs"]
mod scene;
use std::sync::Arc;
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_scene_viewer::{
    RetainedDriver, RetainedEvent, RetainedEventHost, RetainedHost, RetainedInput, RetainedTick,
    RetainedUpdate,
};
use worldrunner_world_simulation::{
    InputAction,
    ui::{UiKey, UiResponse, contribution},
};

impl RetainedDriver for menu::Menu {
    fn update(&mut self, host: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        let layout = self.layout(host.size())?;
        host.set_accessibility(
            self.router
                .semantic_update(&self.world, &layout)
                .map_err(|e| e.to_string())?,
        );
        let mut plan = contribution::extract_ui_layout(&self.world, &self.fonts, layout)
            .map_err(|e| e.to_string())?;
        self.router
            .style_plan(&self.world, &mut plan)
            .map_err(|e| e.to_string())?;
        let overlay = UiOverlay::new(plan).map_err(|e| e.to_string())?;
        host.apply(RetainedUpdate::Ui(Arc::new(overlay)))
            .map_err(|e| e.to_string())
    }
    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        if let RetainedEvent::Input(RetainedInput::Resized(size)) = event {
            self.size = *size;
        }
        if self.size.contains(&0) {
            return Ok(());
        }
        let layout = self.layout(self.size)?;
        let response = match event {
            RetainedEvent::Accessibility(request) => {
                self.router
                    .accessibility_action(&self.world, &layout, *request)
            }
            RetainedEvent::Input(input) => {
                if let Some(InputAction::Pointer(pointer)) = input.simulation_action() {
                    self.router.pointer(&self.world, &layout, pointer)
                } else if let RetainedInput::Key {
                    key,
                    pressed,
                    repeat,
                    ..
                } = input
                {
                    if key == "escape" && *pressed {
                        host.request_exit();
                    }
                    match ui_key(key) {
                        Some(key) => self
                            .router
                            .key(&self.world, &layout, key, *pressed, *repeat),
                        None => Ok(UiResponse::default()),
                    }
                } else {
                    Ok(UiResponse::default())
                }
            }
            _ => Ok(UiResponse::default()),
        }
        .map_err(|e| e.to_string())?;
        self.actions(response.actions)?;
        let layout = self.layout(self.size)?;
        host.set_accessibility(
            self.router
                .semantic_update(&self.world, &layout)
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    }
}
fn ui_key(key: &str) -> Option<UiKey> {
    match key {
        "tab" => Some(UiKey::Tab),
        "shift" => Some(UiKey::Shift),
        "enter" => Some(UiKey::Enter),
        "space" => Some(UiKey::Space),
        "arrowleft" => Some(UiKey::Left),
        "arrowright" => Some(UiKey::Right),
        "arrowup" => Some(UiKey::Up),
        "arrowdown" => Some(UiKey::Down),
        "home" => Some(UiKey::Home),
        "end" => Some(UiKey::End),
        _ => None,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font = std::env::args_os()
        .nth(1)
        .ok_or("pass a TrueType glyf font path to opt in")?;
    let mut menu = menu::Menu::new(std::fs::read(font)?)?;
    let mut host = RetainedHost::new(scene::scene()?, scene::options())?;
    let layout = menu.layout(host.size())?;
    host.set_accessibility(menu.router.semantic_update(&menu.world, &layout)?);
    host.run(&mut menu)?;
    Ok(())
}
