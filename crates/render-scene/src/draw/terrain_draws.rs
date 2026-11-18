//! Whether the stock passes draw the scene's terrain.

/// What the stock passes do with the scene's terrain.
///
/// The terrain is resident either way: it is uploaded, streamed, updated and prepared exactly as
/// without this setting, and other engine code that reads it keeps working. `Suppressed` only
/// stops the stock draws of it, in every pass that has one: the opaque colour pass, the G-buffer
/// and depth prepasses, the metadata prefixes, the capture terms and the shadow casters. A game
/// that draws the terrain with its own passes selects it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainDraws {
    /// The stock passes draw the terrain.
    #[default]
    Stock,
    /// No stock pass draws the terrain; it stays resident.
    Suppressed,
}
