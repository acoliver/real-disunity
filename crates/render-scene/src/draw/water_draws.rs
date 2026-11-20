//! Whether the stock passes draw the scene's water.

/// What the stock passes do with the scene's water.
///
/// The water is resident either way: its bodies are uploaded and its environment and surface
/// inputs are kept and updated exactly as without this setting. `Suppressed` only stops the
/// stock water from taking part in a frame: the frame records no water pass, no reactive mask
/// pass and no water receivers, and the screen-space reflections have no water inputs. A game
/// that draws the water with its own passes selects it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaterDraws {
    /// The stock passes draw the water.
    #[default]
    Stock,
    /// No stock pass draws the water; it stays resident.
    Suppressed,
}

impl WaterDraws {
    /// The water the stock passes draw: `water` itself when they draw it, nothing when they do
    /// not. Every stock consumer of the water reads it through this one rule.
    pub(crate) fn drawn(self, water: Option<&crate::water::Water>) -> Option<&crate::water::Water> {
        water.filter(|_| self == Self::Stock)
    }
}
