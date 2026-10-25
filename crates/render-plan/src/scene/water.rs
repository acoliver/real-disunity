//! Typed water bodies and authored optical settings.
use super::SceneValidationError;
use super::effects::require_range;

/// One axis-aligned water plane rectangle in the east/north plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterBody {
    /// Surface elevation in scene millimetres.
    pub height_millimeters: i64,
    /// Linear scattering colour, in millionths per channel.
    pub color_micro: [u32; 3],
    /// Beer absorption coefficients per metre, in millionths.
    pub absorption_micro: [u32; 3],
    /// Surface roughness in millionths.
    pub roughness_micro: u32,

    /// Signed minimum east corner in millimeters.
    pub east_min_millimeters: i64,
    /// Signed minimum north corner in millimeters.
    pub north_min_millimeters: i64,
    /// Strictly positive east extent in millimeters.
    pub east_extent_millimeters: u64,
    /// Strictly positive north extent in millimeters.
    pub north_extent_millimeters: u64,
}

impl SceneWaterBody {
    /// Validates extents, corner arithmetic and optical quantities.
    /// Colour and roughness use `0..=1_000_000`; absorption uses `0..=10_000_000`.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when an optical quantity is out of range,
    /// an extent is zero or a maximum corner computation overflows.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for value in self.color_micro {
            require_range("water color", u64::from(value), 0, 1_000_000)?;
        }
        for value in self.absorption_micro {
            require_range("water absorption", u64::from(value), 0, 10_000_000)?;
        }
        require_range(
            "water roughness",
            u64::from(self.roughness_micro),
            0,
            1_000_000,
        )?;
        require_extent(
            self.east_extent_millimeters,
            "water east_extent_millimeters",
        )?;
        require_extent(
            self.north_extent_millimeters,
            "water north_extent_millimeters",
        )?;
        checked_corner(
            self.east_min_millimeters,
            self.east_extent_millimeters,
            "water east corner",
        )?;
        checked_corner(
            self.north_min_millimeters,
            self.north_extent_millimeters,
            "water north corner",
        )
    }
}

/// Requires one strictly positive extent.
fn require_extent(extent: u64, part: &'static str) -> Result<(), SceneValidationError> {
    require_range(part, extent, 1, u64::MAX)
}

/// Requires one minimum corner plus extent to stay inside `i64`.
fn checked_corner(
    minimum: i64,
    extent: u64,
    part: &'static str,
) -> Result<(), SceneValidationError> {
    let extent =
        i64::try_from(extent).map_err(|_| SceneValidationError::ArithmeticOverflow { part })?;
    minimum
        .checked_add(extent)
        .ok_or(SceneValidationError::ArithmeticOverflow { part })?;
    Ok(())
}
