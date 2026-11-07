use super::{Content, Renderer, ViewerError};

pub(super) trait Surface {
    fn extent(&self) -> [u32; 2];
    fn window_extent(&self) -> [u32; 2];
    fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError>;
}

impl Surface for Renderer {
    fn extent(&self) -> [u32; 2] {
        self.surface_extent()
    }
    fn window_extent(&self) -> [u32; 2] {
        let size = self
            .window
            .as_ref()
            .expect("windowed renderer")
            .inner_size();
        [size.width, size.height]
    }
    fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        self.resize(size)
    }
}

pub(super) fn notify(content: &mut dyn Content, surface: &impl Surface) {
    content.surface_size(surface.extent());
}

pub(super) fn resize(
    content: &mut dyn Content,
    surface: &mut impl Surface,
    size: [u32; 2],
) -> Result<(), ViewerError> {
    surface.resize(size)?;
    notify(content, surface);
    Ok(())
}

pub(super) fn resume(
    content: &mut dyn Content,
    surface: &mut impl Surface,
) -> Result<(), ViewerError> {
    resize(content, surface, surface.window_extent())
}

#[cfg(test)]
mod tests;
