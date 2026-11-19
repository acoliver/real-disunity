//! Whether the stock passes draw the scene's terrain.

/// What the stock passes do with the scene's terrain.
///
/// The terrain is resident in every variant: it is uploaded, streamed, updated and prepared
/// exactly as without this setting, and other engine code that reads it keeps working. The
/// variants other than `Stock` stop the stock draws of it in the passes that look at the
/// camera: the opaque colour pass, the G-buffer and depth prepasses, the metadata prefixes and
/// the capture terms. They differ in the shadow casters. A game that draws the terrain with its
/// own passes selects one of them.
///
/// The retained shadow cascades hold the terrain casters that the variant selects. Switching
/// between `Stock` and `CastersOnly` leaves the casters unchanged and keeps the retained maps;
/// switching to or from `Suppressed` refreshes every cascade on the next submitted frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainDraws {
    /// The stock passes draw the terrain.
    #[default]
    Stock,
    /// The stock passes draw the terrain only into the shadow cascades. Every camera pass skips
    /// it, and it stays resident. A game that draws the camera-visible terrain itself selects it
    /// to keep the terrain's shadows in the lit frame.
    CastersOnly,
    /// No stock pass draws the terrain, the shadow casters included; it stays resident.
    Suppressed,
}

impl TerrainDraws {
    /// Whether the camera passes draw the terrain: the opaque colour pass, the G-buffer and
    /// depth prepasses, the metadata prefixes and the capture terms.
    #[must_use]
    pub const fn draws_terrain_in_camera_passes(self) -> bool {
        matches!(self, Self::Stock)
    }

    /// Whether the terrain is drawn into the shadow cascades.
    #[must_use]
    pub const fn casts_terrain_shadows(self) -> bool {
        matches!(self, Self::Stock | Self::CastersOnly)
    }
}
