use super::*;
use winit::window::{Window, WindowAttributes, WindowLevel};

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

fn normal_attributes(session: &Session<'_>) -> WindowAttributes {
    session.window_mode().attributes(
        Window::default_attributes()
            .with_title(session.title())
            .with_inner_size(session.window_size_unit().creation_size(session.size()))
            .with_visible(false),
    )
}

#[test]
fn window_level_defaults_preserve_normal_startup_for_both_constructors() {
    for host in hosts() {
        let mut driver = Driver;
        let session = Session::new(host, &mut driver);
        assert!(!session.window_on_top());
        let attributes = crate::app::window_attributes(&session);
        let expected = normal_attributes(&session);
        assert_eq!(format!("{attributes:?}"), format!("{expected:?}"));
        assert_eq!(attributes.window_level, WindowLevel::Normal);
        assert!(!attributes.visible);
    }
}

#[test]
fn window_level_opt_in_changes_only_level_and_preserves_hidden_creation() {
    for host in hosts() {
        let mut driver = Driver;
        let session = Session::new(host.with_window_on_top(true), &mut driver);
        assert!(session.window_on_top());
        let attributes = crate::app::window_attributes(&session);
        let normal = normal_attributes(&session);
        assert_eq!(attributes.window_level, WindowLevel::AlwaysOnTop);
        assert_eq!(attributes.active, normal.active);
        assert_eq!(attributes.visible, normal.visible);
        assert!(!attributes.visible);
        let expected = normal.with_window_level(WindowLevel::AlwaysOnTop);
        assert_eq!(format!("{attributes:?}"), format!("{expected:?}"));
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
        let expected = normal_attributes(&session);
        assert_eq!(format!("{attributes:?}"), format!("{expected:?}"));
        assert_eq!(attributes.window_level, WindowLevel::Normal);
        assert!(!attributes.visible);
    }
}
