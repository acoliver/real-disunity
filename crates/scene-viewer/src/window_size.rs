/// Unit of the initial desktop window request. Resize events always use physical
/// pixels. Windowless hosts start at display scale 1 and interpret both units as pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowSizeUnit {
    #[default]
    Logical,
    Physical,
}

impl WindowSizeUnit {
    pub(crate) fn creation_size(self, [width, height]: [u32; 2]) -> winit::dpi::Size {
        match self {
            Self::Logical => winit::dpi::LogicalSize::new(width, height).into(),
            Self::Physical => winit::dpi::PhysicalSize::new(width, height).into(),
        }
    }
}

/// Desktop window placement. `size` and `window_size_unit` remain the restore size in every
/// mode. The platform reports the resulting surface through ordinary resize events, so the
/// presentation budget always follows the actual physical size. Windowless hosts accept only
/// `Windowed`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMode {
    /// A decorated window at the requested size.
    #[default]
    Windowed,
    /// A decorated window maximized on its monitor.
    Maximized,
    /// An undecorated window covering the monitor the window opens on, at desktop resolution.
    BorderlessFullscreen,
}

impl WindowMode {
    pub(crate) fn attributes(
        self,
        attributes: winit::window::WindowAttributes,
    ) -> winit::window::WindowAttributes {
        attributes
            .with_maximized(self == Self::Maximized)
            .with_fullscreen(self.fullscreen())
    }

    pub(crate) fn fullscreen(self) -> Option<winit::window::Fullscreen> {
        (self == Self::BorderlessFullscreen).then_some(winit::window::Fullscreen::Borderless(None))
    }
}

/// Physical size and scale factor of the monitor a native window occupies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayInfo {
    pub physical_size: [u32; 2],
    pub scale_factor: f64,
}

pub(crate) fn display_info(window: &winit::window::Window) -> Option<DisplayInfo> {
    window.current_monitor().map(|monitor| DisplayInfo {
        physical_size: [monitor.size().width, monitor.size().height],
        scale_factor: monitor.scale_factor(),
    })
}

#[cfg(test)]
mod tests {
    use super::WindowMode;
    use winit::window::{Fullscreen, Window};

    #[test]
    fn windowed_is_the_default_and_requests_neither_maximize_nor_fullscreen() {
        assert_eq!(WindowMode::default(), WindowMode::Windowed);
        let attributes = WindowMode::Windowed.attributes(Window::default_attributes());
        assert!(!attributes.maximized);
        assert!(attributes.fullscreen.is_none());
    }

    #[test]
    fn maximized_requests_only_maximize() {
        let attributes = WindowMode::Maximized.attributes(Window::default_attributes());
        assert!(attributes.maximized);
        assert!(attributes.fullscreen.is_none());
    }

    #[test]
    fn borderless_fullscreen_targets_the_current_monitor() {
        let attributes = WindowMode::BorderlessFullscreen.attributes(Window::default_attributes());
        assert!(!attributes.maximized);
        assert!(matches!(
            attributes.fullscreen,
            Some(Fullscreen::Borderless(None))
        ));
    }
}
