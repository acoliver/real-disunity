//! The screen as a texture the window shows, without a copy through the CPU.
//!
//! A window's frames are submitted by the viewer, so the host cannot tell when they draw into the
//! screen. It says which of the frame's operations write it ([`OffscreenHost::export_screen`]) and
//! gives the texture itself ([`OffscreenHost::screen_texture`]); the viewer decides from what it
//! submitted whether the texture has contents. The one write the host knows of is a flush, which
//! it submits itself.

use worldrunner_render_program::unity::offscreen::{OffscreenError, OffscreenOperations};
use worldrunner_scene_viewer::GAME_SCREEN;

use super::{OffscreenHost, ids};

impl OffscreenHost {
    /// The screen's colour on the device, `None` until a frame or flush made it and after the
    /// screen was taken away. A resize makes a new texture, so a holder of the old one keeps the
    /// old size. The flag records whether a flush has already written this allocation.
    #[must_use]
    pub fn screen_texture(&self) -> Option<(wgpu::Texture, bool)> {
        let handle = self.state.texture_handle(ids::screen_color())?;
        Some((
            self.runtime.gpu_texture(handle).ok()?,
            self.screen_flushed == Some(handle),
        ))
    }

    /// The atlas of the UI frame's runs on the device, `None` until a frame made it and after a
    /// frame left it out. A resize makes a new texture, so a holder of the old one keeps the old
    /// size.
    #[must_use]
    pub fn ui_layers_texture(&self) -> Option<wgpu::Texture> {
        let handle = self
            .state
            .texture_handle(ids::render_texture(crate::offscreen::atlas_id()))?;
        self.runtime.gpu_texture(handle).ok()
    }

    pub(super) fn note_screen_flushed(&mut self) {
        self.screen_flushed = self.state.texture_handle(ids::screen_color());
    }

    /// Names the screen as the viewer's [`GAME_SCREEN`] among the outputs of `operations`, the
    /// operations of the last [`frame`](Self::frame), when they write it. Returns whether they do: operations that
    /// only read the screen or use its depth export nothing, and neither do those of a frame that
    /// failed to build.
    ///
    /// # Errors
    /// Fails when the operations do not write the screen after all, which the export checks.
    pub fn export_screen(
        &self,
        operations: &mut OffscreenOperations,
    ) -> Result<bool, OffscreenError> {
        if !self.frame_writes_screen {
            return Ok(false);
        }
        operations.export(ids::screen_color(), 0, 0, GAME_SCREEN)?;
        Ok(true)
    }
}
