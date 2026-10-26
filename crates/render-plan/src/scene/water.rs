//! Typed water bodies and authored optical settings.
use super::SceneValidationError;
use super::effects::require_range;

/// Surface coverage in the scene east/north plane. Mesh positions are absolute;
/// all surfaces receive the body's elevation in the scene Z-up basis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneWaterSurface {
    Rectangle,
    /// Regular bed samples over the body bounds, row-major from the minimum corner.
    /// Surface triangles are clipped where the linearly interpolated bed is dry.
    Basin {
        columns: u32,
        rows: u32,
        bed_millimeters: Box<[i64]>,
    },
    /// Authored planar triangles; indices must form complete triangles.
    Mesh {
        positions_millimeters: Box<[[i64; 2]]>,
        indices: Box<[u32]>,
    },
}

/// One water body with independent coverage, elevation and optical material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneWaterBody {
    pub surface: SceneWaterSurface,
    pub material: super::SceneWaterSettings,
    /// East/north flow in millimetres per second, applied to procedural waves.
    pub flow_millimeters_per_second: [i32; 2],
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
        self.validate_surface()?;
        super::water_optics::validate(self.material.optics, self.material.shoreline)?;
        if self.material.flow_period_milliseconds == Some(0)
            || (self.material.flow_period_milliseconds.is_some() && self.material.normal.is_none())
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "two-phase water requires a positive period and normal map",
            });
        }
        if self.material.tile_millimeters.contains(&0) {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water tile extent",
            });
        }
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
    fn validate_surface(&self) -> Result<(), SceneValidationError> {
        let invalid = || SceneValidationError::UnsupportedInput {
            part: "water surface geometry",
        };
        match &self.surface {
            SceneWaterSurface::Rectangle => {}
            SceneWaterSurface::Basin {
                columns,
                rows,
                bed_millimeters,
            } => {
                let count = u64::from(*columns) * u64::from(*rows);
                if *columns < 2
                    || *rows < 2
                    || count > 1_000_000
                    || usize::try_from(count).ok() != Some(bed_millimeters.len())
                {
                    return Err(invalid());
                }
            }
            SceneWaterSurface::Mesh {
                positions_millimeters,
                indices,
            } => {
                if positions_millimeters.len() > 1_000_000
                    || indices.len() > 6_000_000
                    || indices.len() % 3 != 0
                    || indices.iter().any(|i| {
                        usize::try_from(*i).map_or(true, |i| i >= positions_millimeters.len())
                    })
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
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
