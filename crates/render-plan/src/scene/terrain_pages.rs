//! Validated configuration for adaptive material pages on a rectangular heightfield.

use thiserror::Error;

/// Authored cache policy. Geometry interval counts remain properties of the heightfield.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainPageSettingsParts {
    /// Interior material texels, independent of geometry sample counts.
    pub interior: [u32; 2],
    /// Extra source texels on every edge, including edges between pages.
    pub border: u32,
    /// Maximum sampling footprint radius in texels at the selected mip.
    pub filter_radius: u32,
    /// Quadtree depth, with the whole rectangular heightfield at depth zero.
    pub maximum_depth: u8,
    /// Split above this projected texel size, in thousandths of a screen pixel.
    pub split_threshold_millipixels: u32,
    /// Merge below this threshold. Must be strictly below the split threshold.
    pub merge_threshold_millipixels: u32,
    /// Hard bound on selected leaves, including the root.
    pub maximum_selected_pages: u32,
    /// Resident allocation ceiling. Includes both four-byte material products and every mip.
    pub resident_bytes: u64,
    /// Maximum complete page uploads admitted per frame, measured in product bytes.
    pub upload_bytes_per_frame: u64,
}

/// Rejected authored page configuration.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TerrainPageSettingsError {
    #[error("page dimensions must be positive and padded dimensions must fit u32")]
    Dimensions,
    #[error("a positive filter radius must fit inside the page border")]
    FilterFootprint,
    #[error("quadtree depth must not exceed 16 and the leaf budget must be positive")]
    SelectionBounds,
    #[error("selection thresholds require 0 < merge < split")]
    Hysteresis,
    #[error("each allocation and upload budget must hold one complete material page")]
    Budget,
}

/// Closed configuration shared by material preparation and scene terrain selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainPageSettings {
    parts: TerrainPageSettingsParts,
    extent: [u32; 2],
    page_bytes: u64,
}

impl TerrainPageSettings {
    pub fn new(parts: TerrainPageSettingsParts) -> Result<Self, TerrainPageSettingsError> {
        let extent = padded_extent(parts)?;
        validate_policy(parts)?;
        let page_bytes = mip_extents(extent)
            .into_iter()
            .try_fold(0_u64, |sum, [w, h]| {
                u64::from(w)
                    .checked_mul(u64::from(h))?
                    .checked_mul(8)?
                    .checked_add(sum)
            })
            .ok_or(TerrainPageSettingsError::Dimensions)?;
        if parts.resident_bytes < page_bytes || parts.upload_bytes_per_frame < page_bytes {
            return Err(TerrainPageSettingsError::Budget);
        }
        Ok(Self {
            parts,
            extent,
            page_bytes,
        })
    }

    pub const fn parts(self) -> TerrainPageSettingsParts {
        self.parts
    }
    pub const fn extent(self) -> [u32; 2] {
        self.extent
    }
    pub const fn page_bytes(self) -> u64 {
        self.page_bytes
    }

    /// Full hardware-compatible chain. Area bins include every odd-edge texel.
    pub fn mip_extents(self) -> Vec<[u32; 2]> {
        mip_extents(self.extent)
    }

    /// Highest mip with aligned neighboring grids and a complete filter gutter.
    ///
    /// Later mips are generated but must not be sampled. A coarser resident
    /// ancestor supplies larger world-space footprints instead. Borders are
    /// composed from neighboring source data, never clamped at internal page edges.
    pub fn maximum_sampling_mip(self) -> u32 {
        let border = u64::from(self.parts.border);
        let radius = u64::from(self.parts.filter_radius);
        let [base_w, base_h] = self.extent;
        let safe = self
            .mip_extents()
            .into_iter()
            .enumerate()
            .take_while(|(level, [w, h])| {
                let step = 1_u64 << level;
                self.extent
                    .into_iter()
                    .chain(self.parts.interior)
                    .all(|size| u64::from(size) % step == 0)
                    && border * u64::from(*w) >= radius * u64::from(base_w)
                    && border * u64::from(*h) >= radius * u64::from(base_h)
            })
            .count();
        u32::try_from(safe - 1).expect("a u32 extent has at most 32 mip levels")
    }
}

fn padded_extent(parts: TerrainPageSettingsParts) -> Result<[u32; 2], TerrainPageSettingsError> {
    let padding = parts
        .border
        .checked_mul(2)
        .ok_or(TerrainPageSettingsError::Dimensions)?;
    let mut extent = parts.interior;
    for size in &mut extent {
        if *size == 0 {
            return Err(TerrainPageSettingsError::Dimensions);
        }
        *size = size
            .checked_add(padding)
            .ok_or(TerrainPageSettingsError::Dimensions)?;
    }
    Ok(extent)
}

fn validate_policy(parts: TerrainPageSettingsParts) -> Result<(), TerrainPageSettingsError> {
    if parts.filter_radius == 0 || parts.filter_radius > parts.border {
        return Err(TerrainPageSettingsError::FilterFootprint);
    }
    if parts.maximum_depth > 16 || parts.maximum_selected_pages == 0 {
        return Err(TerrainPageSettingsError::SelectionBounds);
    }
    if parts.merge_threshold_millipixels == 0
        || parts.merge_threshold_millipixels >= parts.split_threshold_millipixels
    {
        return Err(TerrainPageSettingsError::Hysteresis);
    }
    Ok(())
}

fn mip_extents(mut extent: [u32; 2]) -> Vec<[u32; 2]> {
    let mut result = vec![extent];
    while extent != [1, 1] {
        extent = extent.map(|size| (size / 2).max(1));
        result.push(extent);
    }
    result
}
