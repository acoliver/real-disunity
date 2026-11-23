//! The stock surfaces a game feature draws with its own passes.
use crate::water::Water;

/// The parts of the stock scene that a game feature draws itself. A factory declares it with
/// [`GameFeatureFactory::replaces_stock`](super::GameFeatureFactory::replaces_stock), and a
/// renderer whose inventory has any such factory takes the declared parts out of its stock
/// frames from the moment it is built.
///
/// The scene's terrain and water stay resident either way: they are uploaded, streamed and
/// updated, and the engine code that reads them keeps working. Only the stock draws stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StockReplacement {
    /// The feature draws the scene's terrain surface. Every stock camera pass skips the terrain:
    /// the opaque colour pass, the depth prepass, the metadata prefixes and the capture terms.
    /// The stock shadow casters keep drawing it into the cascades, so the terrain's shadows stay
    /// in the lit frame.
    pub terrain_surface: bool,
    /// The feature draws the scene's water. The frame plans no stock water pass and no water
    /// receivers, and the screen-space reflections get no water inputs. The water bodies and the
    /// water environment stay resident and are lent to the game as without the declaration.
    pub water: bool,
}

impl StockReplacement {
    /// Nothing: the stock passes draw the terrain and the water.
    pub const NONE: Self = Self {
        terrain_surface: false,
        water: false,
    };
    /// The feature draws the scene's terrain surface.
    pub const TERRAIN_SURFACE: Self = Self {
        terrain_surface: true,
        water: false,
    };
    /// The feature draws the scene's water.
    pub const WATER: Self = Self {
        terrain_surface: false,
        water: true,
    };

    /// Everything either declares.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            terrain_surface: self.terrain_surface || other.terrain_surface,
            water: self.water || other.water,
        }
    }

    /// The water the stock passes draw: `water` itself unless a feature draws it. Every stock
    /// consumer of the water reads it through this one rule.
    pub(crate) fn stock_water(self, water: Option<&Water>) -> Option<&Water> {
        water.filter(|_| !self.water)
    }
}
