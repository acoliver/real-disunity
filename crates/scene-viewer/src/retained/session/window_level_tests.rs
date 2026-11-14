use super::*;
use winit::window::{Window, WindowLevel};

struct Driver;
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        Ok(())
    }
}

fn hosts() -> [RetainedHost; 2] {
    [
        RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap(),
        RetainedHost::with_initial_content(crate::tests::options(), |_| {
            Ok(crate::InitialContent {
                scene: crate::tests::scene(false),
                settings: None,
            })
        })
        .unwrap(),
    ]
}

#[test]
fn window_level_defaults_preserve_normal_startup_for_both_constructors() {
    let defaults = Window::default_attributes();
    for host in hosts() {
        let mut driver = Driver;
        let session = Session::new(host, &mut driver);
        assert!(!session.window_on_top());
        let attributes = crate::app::window_attributes(&session);
        assert_eq!(attributes.window_level, defaults.window_level);
        assert_eq!(attributes.active, defaults.active);
        assert!(!attributes.visible);
    }
}

#[test]
fn window_level_opt_in_requests_on_top_without_initial_focus() {
    for host in hosts() {
        let mut driver = Driver;
        let session = Session::new(host.with_window_on_top(true), &mut driver);
        assert!(session.window_on_top());
        let attributes = crate::app::window_attributes(&session);
        assert_eq!(attributes.window_level, WindowLevel::AlwaysOnTop);
        assert!(!attributes.active);
        assert!(attributes.visible);
        assert_eq!(attributes.title, session.title());
        assert_eq!(
            attributes.inner_size,
            Some(session.window_size_unit().creation_size(session.size()))
        );
    }
}

#[test]
fn window_level_explicit_false_restores_normal_startup() {
    for host in hosts() {
        let mut driver = Driver;
        let session = Session::new(
            host.with_window_on_top(true).with_window_on_top(false),
            &mut driver,
        );
        assert!(!session.window_on_top());
        let attributes = crate::app::window_attributes(&session);
        assert_eq!(attributes.window_level, WindowLevel::Normal);
        assert!(attributes.active);
        assert!(!attributes.visible);
    }
}
