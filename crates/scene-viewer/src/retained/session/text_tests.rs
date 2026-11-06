use super::*;
use crate::retained::TextInput;
use crate::retained::input::text::keyboard_text;
use crate::retained::ui_font;
use std::{cell::RefCell, rc::Rc};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldSettings};
struct Driver {
    events: Rc<RefCell<Vec<RetainedInput>>>,
    world: World,
    fonts: FontBook,
    router: UiRouter,
}
impl Driver {
    fn new(events: Rc<RefCell<Vec<RetainedInput>>>) -> Self {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                object,
                ComponentDesc::RectTransform(RectTransform::default()),
            )
            .unwrap();
        world
            .add_component(object, ComponentDesc::Canvas(Canvas::default()))
            .unwrap();
        let mut fonts = FontBook::new(UiLimits::default()).unwrap();
        let run = TextRun::new(fonts.register(ui_font::font()).unwrap());
        let mut router = UiRouter::default();
        router.controls.insert(object, UiControl::text_field());
        router
            .text_fields
            .insert(object, UiTextField::new(run, 32, &fonts).unwrap());
        Self {
            events,
            world,
            fonts,
            router,
        }
    }
}
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        Ok(())
    }
    fn event(
        &mut self,
        host: &mut crate::RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        if let RetainedEvent::Input(event) = event {
            self.events.borrow_mut().push(event.clone());
            let layout = self
                .world
                .ui_layout(host.size().into())
                .map_err(|e| e.to_string())?;
            match event {
                RetainedInput::Key { pressed: true, .. } => {
                    self.router
                        .key(&self.world, &layout, UiKey::Tab, true, false)
                        .map_err(|e| e.to_string())?;
                }
                RetainedInput::Text(text) => {
                    self.router
                        .text(&self.world, &layout, &self.fonts, text)
                        .map_err(|e| e.to_string())?;
                }
                RetainedInput::PointerCancelled => {
                    self.router
                        .pointer(
                            &self.world,
                            &layout,
                            worldrunner_world_simulation::PointerEvent::Cancel,
                        )
                        .map_err(|e| e.to_string())?;
                }
                _ => {}
            }
            let area = self
                .router
                .text_input_area(&self.world, &layout, &self.fonts)
                .map_err(|e| e.to_string())?;
            host.set_text_input_area(area).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
#[test]
fn text_repeat_ime_and_focus_use_the_session_without_a_window() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut driver = Driver::new(events.clone());
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut session = Session::new(host, &mut driver);
    assert_eq!(session.host.text_input_area(), None);
    session.input(super::super::input::key_input(
        &crate::LogicalKey::Character("x".into()),
        crate::KeyCode::KeyX.into(),
        winit::event::ElementState::Pressed,
        false,
    ));
    assert!(session.host.text_input_area().is_some());
    for _ in 0..3 {
        session.committed_key_text(Some("X"), true);
    }
    session.event(&WindowEvent::Ime(winit::event::Ime::Enabled), None);
    session.event(
        &WindowEvent::Ime(winit::event::Ime::Preedit("A".into(), Some((1, 1)))),
        None,
    );
    session.committed_key_text(Some("A"), true);
    session.event(
        &WindowEvent::Ime(winit::event::Ime::Commit("A".into())),
        None,
    );
    session.event(&WindowEvent::Ime(winit::event::Ime::Disabled), None);
    let commits: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            RetainedInput::Text(TextInput::Commit(t)) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(commits, ["X", "X", "X", "A"]);
    session.input(RetainedInput::PointerCancelled);
    assert_eq!(session.host.text_input_area(), None);
    assert_eq!(keyboard_text(Some("x"), false), None);
    assert_eq!(keyboard_text(Some("\r"), true), None);
}
#[test]
fn resize_observations_carry_the_current_display_scale() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut driver = Driver::new(events.clone());
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut session = Session::new(host, &mut driver);
    session.input(RetainedInput::DisplayChanged {
        scale_factor: 2.,
        physical_size: [800, 600],
    });
    session.surface_size([640, 480]);
    assert_eq!(session.host.scale_factor().to_bits(), 2_f64.to_bits());
    assert_eq!(session.host.size(), [640, 480]);
    assert!(events.borrow().contains(&RetainedInput::DisplayChanged {
        scale_factor: 2.,
        physical_size: [640, 480]
    }));
}
