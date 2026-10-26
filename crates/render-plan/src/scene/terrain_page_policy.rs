//! Scene-owned residency admission for layered terrain material pages.
use super::{TerrainPageSettings, TerrainPageSettingsError};

/// Absence on terrain disables residency. All allocation and selection bounds are explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainPagePolicy {
    settings: TerrainPageSettings,
    slots: u32,
    budget_bytes: u64,
}
impl TerrainPagePolicy {
    /// Requires enough slots for the complete bounded selection tree.
    /// The renderer additionally validates scratch, table and device allocation limits.
    pub fn new(
        settings: TerrainPageSettings,
        slots: u32,
        budget_bytes: u64,
    ) -> Result<Self, TerrainPageSettingsError> {
        let parts = settings.parts();
        let leaves = 1 + (u64::from(parts.maximum_selected_pages) - 1) / 3 * 3;
        let nodes = ((leaves * 4 - 1) / 3)
            .min(((1_u64 << (2 * (u32::from(parts.maximum_depth) + 1))) - 1) / 3);
        if u64::from(slots) < nodes {
            return Err(TerrainPageSettingsError::SelectionBounds);
        }
        if budget_bytes < settings.page_bytes() || slots == 0 {
            return Err(TerrainPageSettingsError::Budget);
        }
        Ok(Self {
            settings,
            slots,
            budget_bytes,
        })
    }

    pub const fn settings(self) -> TerrainPageSettings {
        self.settings
    }
    pub const fn slots(self) -> u32 {
        self.slots
    }
    pub const fn budget_bytes(self) -> u64 {
        self.budget_bytes
    }
}
