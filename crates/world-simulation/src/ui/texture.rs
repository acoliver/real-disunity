//! What a UI draw samples.
use crate::graphics::RenderTextureId;

/// The texture a UI draw reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiTexture {
    /// The renderer identity of a stored image the managed host registered.
    Image(u64),
    /// A key into `UiPresentation::coverage`; only text draws use it.
    Coverage(u64),
    /// A world render texture; the player supplies its GPU texture.
    Target(RenderTextureId),
}
