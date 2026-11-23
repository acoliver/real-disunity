//! The game's own screen as the retained viewer shows it.
//!
//! A player draws the game into a render texture on the viewer's device. The host offers that
//! texture to the viewer ([`crate::RetainedHost::show_game_screen`]) and, in frames whose
//! operations draw into it, exports it from them under [`GAME_SCREEN`]. A frame then shows the
//! screen in place of the stock scene and the UI overlay draws over it. Nothing is read back to
//! the CPU.
//!
//! The viewer decides per frame whether the screen has contents to show ([`Ledger`]). It counts
//! a write as done only once the frame that carried it was submitted, so operations that were
//! replaced before a frame took them, or a frame that failed to encode, leave the screen unwritten.

use crate::ViewerError;
use worldrunner_render_post::ResourceRef;

/// The name the operations that draw the screen export it under, and the frame plan reads it by.
pub const GAME_SCREEN: ResourceRef = ResourceRef::new("game-screen");

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Checks that `texture` is a single-sample `Rgba8UnormSrgb` texture the viewer can sample, the
/// form of every render texture a host offers, and fails with `reason` otherwise.
pub(crate) fn require_render_texture(
    texture: &wgpu::Texture,
    reason: &'static str,
) -> Result<(), ViewerError> {
    let sampled = texture
        .usage()
        .contains(wgpu::TextureUsages::TEXTURE_BINDING);
    if texture.format() != FORMAT || texture.sample_count() != 1 || !sampled {
        return Err(ViewerError::UnsupportedScene(reason));
    }
    Ok(())
}

/// The game's screen texture and what the host knows of its contents.
#[derive(Clone, Debug)]
pub struct GameScreen {
    texture: wgpu::Texture,
    submitted: bool,
}

impl GameScreen {
    /// A screen of `texture`, which must be a single-sample `Rgba8UnormSrgb` texture the viewer can
    /// sample, its rows bottom first as a render texture's are. `submitted` says that work the
    /// host already submitted, such as a synchronous flush, wrote into this very texture. Work of
    /// frames handed to the viewer does not count: the viewer knows whether it ran.
    ///
    /// # Errors
    /// Fails by name when the texture is not such a texture.
    pub fn new(texture: wgpu::Texture, submitted: bool) -> Result<Self, ViewerError> {
        require_render_texture(
            &texture,
            "a game screen that is not a single-sample sampled Rgba8UnormSrgb texture",
        )?;
        Ok(Self { texture, submitted })
    }

    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    #[must_use]
    pub fn submitted(&self) -> bool {
        self.submitted
    }
}

/// What a frame shows of the offered screen.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Show<'a, K> {
    pub screen: &'a K,
    /// Whether submitted work already wrote the screen. When it did not, the frame's own
    /// operations write it.
    pub retained: bool,
}

/// Which writes the offered screen has received, by the frame that carried them.
///
/// `K` identifies a screen's storage. A different `K` is another allocation, such as the one a
/// resize makes, and starts with no contents.
#[derive(Debug)]
pub(crate) struct Ledger<K> {
    screen: Option<K>,
    written: bool,
    pending: bool,
}

impl<K> Default for Ledger<K> {
    fn default() -> Self {
        Self {
            screen: None,
            written: false,
            pending: false,
        }
    }
}

impl<K: PartialEq + Clone> Ledger<K> {
    /// The screen the host offers for the next frame, with whether it says submitted work wrote
    /// it, or `None` when it offers none. A write a frame encoded but that was not submitted is
    /// forgotten. Returns whether cached bindings to the old allocation must be dropped.
    pub(crate) fn offer(&mut self, screen: Option<(&K, bool)>) -> bool {
        self.pending = false;
        let Some((key, submitted)) = screen else {
            let changed = self.screen.take().is_some();
            self.written = false;
            return changed;
        };
        if self.screen.as_ref() == Some(key) {
            self.written |= submitted;
            return false;
        }
        self.screen = Some(key.clone());
        self.written = submitted;
        true
    }

    /// The allocation the host currently offers.
    pub(crate) fn offered(&self) -> Option<&K> {
        self.screen.as_ref()
    }

    /// Whether an earlier submission wrote the currently offered allocation.
    pub(crate) fn has_submitted_screen(&self) -> bool {
        self.written
    }

    /// Predicts frame selection for capture admission without recording a pending write.
    pub(crate) fn will_show(&self, exported: bool) -> bool {
        self.screen.is_some() && (self.written || exported)
    }

    /// What the frame about to be encoded shows. `exported` is whether its operations export the
    /// screen, that is whether they write it this frame. `None` means the frame shows the stock
    /// scene: no screen is offered, or nothing wrote it.
    pub(crate) fn plan(&mut self, exported: bool) -> Option<Show<'_, K>> {
        self.pending = exported && self.screen.is_some();
        if !self.will_show(exported) {
            return None;
        }
        Some(Show {
            screen: self.screen.as_ref()?,
            retained: self.written,
        })
    }

    /// The planned frame was submitted: its write, if it had one, happened.
    pub(crate) fn submitted(&mut self) {
        self.written |= std::mem::take(&mut self.pending);
    }
}

#[cfg(test)]
#[path = "game_screen_tests.rs"]
mod tests;
