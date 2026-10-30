/// Unit of the initial desktop window request. Resize events always use physical
/// pixels. Windowless hosts have no display scale and interpret both units as pixels.
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
