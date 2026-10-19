//! Authored terrain geometry and material palette recipe.
//!
//! The recipe describes exact heightfield geometry and authored palette
//! declarations; sample data itself lives behind the referenced height
//! asset, so no byte layout or dataset dimension is embedded here. The
//! palette declares which materials exist and how they tile; this layer
//! carries no per-sample material grid.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::validate::{
    checked_add_i64, checked_mul_u64, require_positive_u32, require_range, require_sorted_unique,
    require_uri,
};

/// One authored terrain material palette declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainMaterialSlot {
    /// Authored palette slot index; no material grid is carried in this layer.
    pub slot_index: u16,
    /// Structural URI of the material asset bound to this slot.
    pub material_uri: String,
    /// Texture tiling extent in millimeters; strictly positive.
    pub uv_scale_millimeters: u32,
}

impl TerrainMaterialSlot {
    /// Returns the palette slot index.
    #[must_use]
    pub const fn slot_index(&self) -> u16 {
        self.slot_index
    }

    /// Returns the structural URI of the bound material asset.
    #[must_use]
    pub fn material_uri(&self) -> &str {
        &self.material_uri
    }

    /// Validates the slot URI and tiling extent.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the URI is malformed or the
    /// tiling extent is zero.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("terrain material URI", &self.material_uri)?;
        require_positive_u32("terrain uv_scale_millimeters", self.uv_scale_millimeters)
    }
}

/// Authored terrain heightfield recipe.
///
/// Geometry is a regular sample grid with whole-millimeter spacing and
/// origin; the east/north plane is terrain-local, with elevation carried by
/// the referenced height asset content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainRecipe {
    /// Structural URI of the heightfield asset this recipe references.
    pub height_asset_uri: String,
    /// Sample column count; at least two samples per edge.
    pub columns: u32,
    /// Sample row count; at least two samples per edge.
    pub rows: u32,
    /// Distance between adjacent samples in millimeters; strictly positive.
    pub sample_spacing_millimeters: u32,
    /// Signed origin offset east of the terrain-local origin.
    pub origin_east_millimeters: i64,
    /// Signed origin offset north of the terrain-local origin.
    pub origin_north_millimeters: i64,
    /// Material palette declarations in strictly ascending, duplicate-free
    /// slot order.
    pub material_palette: Vec<TerrainMaterialSlot>,
}

impl TerrainRecipe {
    /// Returns the structural URI of the heightfield asset.
    #[must_use]
    pub fn height_asset_uri(&self) -> &str {
        &self.height_asset_uri
    }

    /// Returns the strictly ordered material palette.
    #[must_use]
    pub fn material_palette(&self) -> &[TerrainMaterialSlot] {
        &self.material_palette
    }

    /// Validates geometry and palette closure.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when dimensions are below two, the
    /// spacing is zero, or the palette is empty, unsorted, duplicated, or
    /// individually malformed.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("terrain height asset_uri", &self.height_asset_uri)?;
        require_range(
            "terrain columns",
            2,
            i128::from(u32::MAX),
            i128::from(self.columns),
        )?;
        require_range(
            "terrain rows",
            2,
            i128::from(u32::MAX),
            i128::from(self.rows),
        )?;
        require_positive_u32(
            "terrain sample_spacing_millimeters",
            self.sample_spacing_millimeters,
        )?;
        if self.material_palette.is_empty() {
            return Err(ContractDocumentError::EmptyCollection {
                field: "terrain material_palette",
            });
        }
        require_sorted_unique(
            "terrain material_palette",
            self.material_palette
                .iter()
                .map(|slot| u64::from(slot.slot_index)),
        )?;
        for slot in &self.material_palette {
            slot.validate()?;
        }
        Ok(())
    }

    /// Returns the checked sample count of the height grid.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when
    /// `columns * rows` overflows `u64`.
    pub fn sample_count(&self) -> Result<u64, ContractDocumentError> {
        checked_mul_u64(
            "terrain sample count",
            u64::from(self.columns),
            u64::from(self.rows),
        )
    }

    /// Returns the checked east edge coordinate, origin included.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the edge
    /// computation overflows.
    pub fn east_max_millimeters(&self) -> Result<i64, ContractDocumentError> {
        let extent = self.east_extent_millimeters()?;
        checked_add_i64(
            "terrain east edge",
            self.origin_east_millimeters,
            i64::try_from(extent).map_err(|_| ContractDocumentError::ArithmeticOverflow {
                field: "terrain east edge",
            })?,
        )
    }

    /// Returns the checked north edge coordinate, origin included.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::ArithmeticOverflow`] when the edge
    /// computation overflows.
    pub fn north_max_millimeters(&self) -> Result<i64, ContractDocumentError> {
        let extent = self.north_extent_millimeters()?;
        checked_add_i64(
            "terrain north edge",
            self.origin_north_millimeters,
            i64::try_from(extent).map_err(|_| ContractDocumentError::ArithmeticOverflow {
                field: "terrain north edge",
            })?,
        )
    }

    /// Returns the checked east extent between the outermost samples.
    fn east_extent_millimeters(&self) -> Result<u64, ContractDocumentError> {
        let intervals = u64::from(self.columns).checked_sub(1).ok_or(
            ContractDocumentError::ArithmeticOverflow {
                field: "terrain east extent",
            },
        )?;
        checked_mul_u64(
            "terrain east extent",
            intervals,
            u64::from(self.sample_spacing_millimeters),
        )
    }

    /// Returns the checked north extent between the outermost samples.
    fn north_extent_millimeters(&self) -> Result<u64, ContractDocumentError> {
        let intervals = u64::from(self.rows).checked_sub(1).ok_or(
            ContractDocumentError::ArithmeticOverflow {
                field: "terrain north extent",
            },
        )?;
        checked_mul_u64(
            "terrain north extent",
            intervals,
            u64::from(self.sample_spacing_millimeters),
        )
    }
}
