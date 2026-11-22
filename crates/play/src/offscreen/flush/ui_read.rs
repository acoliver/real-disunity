//! A render texture the UI draws, as the window or a screen capture shows it.
//!
//! The UI reads render textures outside the frame plan: the host gives the GPU texture of the
//! state, the overlay samples it after the frame's offscreen work, and the encoder order puts
//! that work first. What the host answers is whether a frame the state built wrote the texture,
//! which is the one fact the draws' `unwritten-render-texture` stand-in already reads.

use worldrunner_world_simulation::graphics::RenderTextureId;

use super::{OffscreenHost, ids};

/// What the host holds of one render texture the UI reads.
#[derive(Clone, Debug)]
pub struct UiRead {
    /// The texture on the device, `None` while no frame has made one.
    pub texture: Option<wgpu::Texture>,
    /// Whether a frame the state built wrote it. A texture made again, after a resize or after its
    /// storage was given back, is not written until a frame draws it.
    pub written: bool,
}

impl OffscreenHost {
    /// The device texture of the render texture `id` and whether it was written, and notes the
    /// read so that giving storage back for room treats the UI's read like a frame naming it.
    pub fn render_texture_for_ui(&mut self, id: RenderTextureId) -> UiRead {
        let texture_id = ids::render_texture(id);
        self.adapter.note_ui_read(texture_id);
        let texture = self
            .state
            .texture_handle(texture_id)
            .and_then(|handle| self.runtime.gpu_texture(handle).ok());
        let written = texture.is_some() && !self.state.render_texture_is_unwritten(texture_id);
        UiRead { texture, written }
    }
}
