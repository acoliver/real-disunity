//! Owned renderer-domain terrain: heights, surface grid, and surfaces.
//!
//! The terrain is closed CPU state: a regular millimeter heightfield grid,
//! one surface index per sample, and a dense list of surfaces. Sample
//! counts, surface indices, and handle references are validated by
//! construction; no path, dataset dimension, or default exists here.
//!
//! The grid keys surfaces rather than materials because the authored
//! relation from source terrain index to material is many-to-one, and the
//! duplicates are told apart by appearance alone. See
//! [`SceneTerrainSurface`].
//!
//! # Canonical grid orientation
//!
//! Every grid in this module is canonical, not merely row-major: the column
//! index advances east and the row index advances north, each by exactly the
//! sample spacing, and the origin is the world position of sample
//! (column 0, row 0). Producers whose source data runs the other way normalise
//! onto that orientation before building a terrain; consumers may rely on it
//! without asking where the samples came from.

use crate::scene::error::SceneValidationError;
use crate::scene::resource::MaterialHandle;

/// Closed authored terrain level-of-detail policy.
///
/// The renderer semantics of each policy are fixed here so authored terrain
/// carries no renderer interpretation of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainLodPolicy {
    /// Every sample renders at full resolution; no reduction is applied.
    FullResolution,
    /// The grid halves `steps` times after its base level; strictly positive
    /// and bounded by the number of reductions the grid supports.
    UniformSteps { steps: u8 },
}

/// Per-surface appearance modifiers applied on top of a material.
///
/// This duplicates the shape of the authored terrain appearance rather than
/// importing it, because this module is a closed renderer domain that must
/// not depend on the authoring schema: an authority type here would let a
/// change to the document format reach the renderer without passing through
/// composition.
///
/// [`Self::NEUTRAL`] is the identity, and its tint is full percent per
/// channel rather than zero, because the tint is a multiplier - a zero tint
/// would render the surface black while still validating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTerrainAppearance {
    /// Optional height-aware texture layer. Requires a layered terrain source.
    pub layered: Option<super::TerrainLayer>,
    /// Radius in millimeters over which the surface's coverage mask is
    /// softened against its neighbours; zero leaves the mask hard.
    pub blur_radius_millimeters: u32,
    /// Per-channel colour multiplier in percent, ordered red, green, blue.
    pub tint_percent: [u16; 3],
    /// Rotation of the surface's detail mapping in arcminutes, measured
    /// about the world origin rather than about any texture coordinate.
    pub rotation_arcminutes: u32,
}

impl SceneTerrainAppearance {
    /// The identity appearance: no blur, no tint, no rotation.
    pub const NEUTRAL: Self = Self {
        layered: None,
        blur_radius_millimeters: 0,
        tint_percent: [100; 3],
        rotation_arcminutes: 0,
    };
}

/// One distinct terrain surface: a material together with its appearance.
///
/// A surface is deliberately not a palette entry. The authored relation from
/// source terrain index to material is many-to-one, and the duplicates are
/// distinguished by appearance alone - several indices can bind the same
/// grass material and differ only in tint and rotation. Keying the grid by
/// material would therefore erase precisely the distinction the appearance
/// exists to express, so the grid keys surfaces and one material may back
/// several of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTerrainSurface {
    material: MaterialHandle,
    appearance: SceneTerrainAppearance,
}

impl SceneTerrainSurface {
    /// Pairs one material with the appearance that distinguishes this
    /// surface from others backed by the same material.
    #[must_use]
    pub const fn new(material: MaterialHandle, appearance: SceneTerrainAppearance) -> Self {
        Self {
            material,
            appearance,
        }
    }

    /// Returns the material backing this surface.
    #[must_use]
    pub const fn material(&self) -> MaterialHandle {
        self.material
    }

    /// Returns the appearance distinguishing this surface.
    #[must_use]
    pub const fn appearance(&self) -> SceneTerrainAppearance {
        self.appearance
    }
}

/// Complete owned inputs for one closed terrain grid.
///
/// # Canonical grid orientation
///
/// The grid is canonical, not merely row-major. The column index advances
/// **east** and the row index advances **north**, each by exactly
/// `sample_spacing_millimeters`, and the origin is the world position of
/// sample (column 0, row 0). Sample `(c, r)` therefore sits at
/// `origin_east_millimeters + c * sample_spacing_millimeters` east of the
/// scene origin and `origin_north_millimeters + r * sample_spacing_millimeters`
/// north of it. Because the spacing is a single positive magnitude, no axis
/// can express a westward or southward step.
///
/// Nothing here can detect a violated orientation - a mirrored grid validates
/// exactly like a correct one - so a producer whose source samples run west or
/// south must reverse that axis before building these parts, reversing the
/// height and material grids identically, and must take the origin from the
/// far end of the source window rather than its near end.
pub struct SceneTerrainParts {
    /// Sample column count; the column index advances east.
    pub columns: u32,
    /// Sample row count; the row index advances north.
    pub rows: u32,
    /// Signed east offset from the scene origin of sample (column 0, row 0).
    ///
    /// Sample `(c, r)` lies `c * sample_spacing_millimeters` further east, so
    /// this is the westmost sample column of the grid.
    pub origin_east_millimeters: i64,
    /// Signed north offset from the scene origin of sample (column 0, row 0).
    ///
    /// Sample `(c, r)` lies `r * sample_spacing_millimeters` further north, so
    /// this is the southmost sample row of the grid.
    pub origin_north_millimeters: i64,
    /// Distance between adjacent samples in millimeters, applied identically
    /// eastward along the column axis and northward along the row axis.
    pub sample_spacing_millimeters: u32,
    /// Whole-millimeter height samples in canonical row-major order: rows
    /// advance north, and within a row columns advance east.
    pub heights_millimeters: Vec<i32>,
    /// Per-sample surface indices in the same canonical row-major order as
    /// `heights_millimeters`, one for one.
    pub surface_indices: Vec<u8>,
    /// Dense surfaces referenced by the surface indices. One material may
    /// back several entries when they differ in appearance.
    pub surfaces: Vec<SceneTerrainSurface>,
    /// Authored level-of-detail policy.
    pub lod_policy: TerrainLodPolicy,
}

/// One owned terrain in canonical renderer-domain units.
///
/// The grid orientation is the canonical one described on
/// [`SceneTerrainParts`]: the column index advances east, the row index
/// advances north, both by [`Self::sample_spacing_millimeters`], and
/// [`Self::origin_east_millimeters`] with [`Self::origin_north_millimeters`]
/// give the world position of sample (column 0, row 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneTerrain {
    columns: u32,
    rows: u32,
    origin_east_millimeters: i64,
    origin_north_millimeters: i64,
    sample_spacing_millimeters: u32,
    heights_millimeters: Box<[i32]>,
    elevation_bounds: [i32; 2],
    surface_indices: Box<[u8]>,
    surfaces: Box<[SceneTerrainSurface]>,
    lod_policy: TerrainLodPolicy,
    layered_source: Option<super::TerrainLayeredSource>,
    page_policy: Option<super::TerrainPagePolicy>,
    streaming_policy: Option<super::TerrainStreamingPolicy>,
}

impl SceneTerrain {
    /// Adopts one terrain after validating grid, palette, and policy closure.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the dimensions are below two,
    /// the spacing is zero, the height or material grid does not carry
    /// exactly `columns * rows` samples, a material index lies outside the
    /// palette, or the LOD policy leaves its addressable range.
    pub fn new(parts: SceneTerrainParts) -> Result<Self, SceneValidationError> {
        let elevation_bounds = [
            parts.heights_millimeters.iter().copied().min().unwrap_or(0),
            parts.heights_millimeters.iter().copied().max().unwrap_or(0),
        ];
        let terrain = Self {
            elevation_bounds,
            columns: parts.columns,
            rows: parts.rows,
            origin_east_millimeters: parts.origin_east_millimeters,
            origin_north_millimeters: parts.origin_north_millimeters,
            sample_spacing_millimeters: parts.sample_spacing_millimeters,
            heights_millimeters: parts.heights_millimeters.into_boxed_slice(),
            surface_indices: parts.surface_indices.into_boxed_slice(),
            surfaces: parts.surfaces.into_boxed_slice(),
            lod_policy: parts.lod_policy,
            layered_source: None,
            page_policy: None,
            streaming_policy: None,
        };
        terrain.validate()?;
        Ok(terrain)
    }

    /// Adopts validated layered composition with an independent material-cell grid.
    pub fn with_layered_source(
        mut self,
        source: super::TerrainLayeredSource,
    ) -> Result<Self, SceneValidationError> {
        source.validate(&self)?;
        self.layered_source = Some(source);
        Ok(self)
    }

    pub const fn layered_source(&self) -> Option<&super::TerrainLayeredSource> {
        self.layered_source.as_ref()
    }

    /// Enables bounded material residency for this terrain's layered source.
    /// Passing `None` disables material pages.
    pub fn with_page_policy(
        mut self,
        policy: Option<super::TerrainPagePolicy>,
    ) -> Result<Self, SceneValidationError> {
        if policy.is_some() && self.layered_source.is_none() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "terrain page policy requires a layered source",
            });
        }
        self.page_policy = policy;
        Ok(self)
    }

    pub const fn page_policy(&self) -> Option<super::TerrainPagePolicy> {
        self.page_policy
    }

    /// Enables bounded quadtree geometry with skirted borders. The uniform LOD
    /// policy remains unchanged when streaming is disabled.
    #[must_use]
    pub fn with_streaming_policy(mut self, policy: Option<super::TerrainStreamingPolicy>) -> Self {
        self.streaming_policy = policy;
        self
    }

    pub const fn streaming_policy(&self) -> Option<super::TerrainStreamingPolicy> {
        self.streaming_policy
    }

    /// Returns the sample column count, counted along the eastward axis.
    #[must_use]
    pub const fn columns(&self) -> u32 {
        self.columns
    }

    /// Returns the sample row count, counted along the northward axis.
    #[must_use]
    pub const fn rows(&self) -> u32 {
        self.rows
    }

    /// Returns the signed east offset from the scene origin of sample
    /// (column 0, row 0), which is the westmost sample column.
    ///
    /// Sample `(c, r)` lies `c * sample_spacing_millimeters` further east.
    #[must_use]
    pub const fn origin_east_millimeters(&self) -> i64 {
        self.origin_east_millimeters
    }

    /// Returns the signed north offset from the scene origin of sample
    /// (column 0, row 0), which is the southmost sample row.
    ///
    /// Sample `(c, r)` lies `r * sample_spacing_millimeters` further north.
    #[must_use]
    pub const fn origin_north_millimeters(&self) -> i64 {
        self.origin_north_millimeters
    }

    /// Returns the distance between adjacent samples in millimeters, applied
    /// identically eastward along columns and northward along rows.
    #[must_use]
    pub const fn sample_spacing_millimeters(&self) -> u32 {
        self.sample_spacing_millimeters
    }

    /// Returns the owned whole-millimeter height samples in canonical
    /// row-major order: rows advance north, columns advance east.
    #[must_use]
    pub fn heights_millimeters(&self) -> &[i32] {
        &self.heights_millimeters
    }

    /// Inclusive minimum and maximum authored height, in millimeters.
    pub const fn elevation_bounds(&self) -> [i32; 2] {
        self.elevation_bounds
    }

    /// Returns the owned per-sample surface indices in the same canonical
    /// row-major order as the height samples, one for one.
    #[must_use]
    pub fn surface_indices(&self) -> &[u8] {
        &self.surface_indices
    }

    /// Returns the owned dense surfaces addressed by the surface indices.
    #[must_use]
    pub fn surfaces(&self) -> &[SceneTerrainSurface] {
        &self.surfaces
    }

    /// Returns the authored level-of-detail policy.
    #[must_use]
    pub const fn lod_policy(&self) -> TerrainLodPolicy {
        self.lod_policy
    }

    /// Validates grid geometry, sample counts, palette indices, and policy.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        const PART: &str = "terrain";
        if let Some(source) = &self.layered_source {
            source.validate(self)?;
        }
        for surface in &self.surfaces {
            if let Some(layer) = surface.appearance().layered {
                layer.validate()?;
            }
        }
        if self.columns < 2 || self.rows < 2 {
            return Err(SceneValidationError::InvalidDimensions {
                part: PART,
                width: self.columns,
                height: self.rows,
            });
        }
        if self.sample_spacing_millimeters == 0 {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "terrain sample_spacing_millimeters",
            });
        }
        let expected = sample_count(self.columns, self.rows)?;
        require_sample_count("terrain heights", &self.heights_millimeters, expected)?;
        require_sample_count("terrain surface indices", &self.surface_indices, expected)?;
        if self.surfaces.is_empty() {
            return Err(SceneValidationError::EmptyCollection {
                part: "terrain surfaces",
            });
        }
        let surface_limit = u64::try_from(self.surfaces.len())
            .map_err(|_| SceneValidationError::ArithmeticOverflow { part: PART })?;
        for index in &self.surface_indices {
            if u64::from(*index) >= surface_limit {
                return Err(SceneValidationError::MaterialIndexOutOfRange {
                    part: "terrain surface indices",
                    index: *index,
                    limit: surface_limit,
                });
            }
        }
        validate_lod_policy(self.lod_policy, self.columns, self.rows)
    }
}

/// Requires one grid to carry exactly the terrain sample count.
fn require_sample_count<T>(
    part: &'static str,
    grid: &[T],
    expected: u64,
) -> Result<(), SceneValidationError> {
    let observed =
        u64::try_from(grid.len()).map_err(|_| SceneValidationError::ArithmeticOverflow { part })?;
    if observed == expected {
        Ok(())
    } else {
        Err(SceneValidationError::TerrainSampleCount {
            part,
            expected,
            observed,
        })
    }
}

/// Computes `columns * rows` with checked arithmetic.
fn sample_count(columns: u32, rows: u32) -> Result<u64, SceneValidationError> {
    u64::from(columns).checked_mul(u64::from(rows)).ok_or(
        SceneValidationError::ArithmeticOverflow {
            part: "terrain sample count",
        },
    )
}

/// Validates one LOD policy against the grid it serves.
fn validate_lod_policy(
    policy: TerrainLodPolicy,
    columns: u32,
    rows: u32,
) -> Result<(), SceneValidationError> {
    let TerrainLodPolicy::UniformSteps { steps } = policy else {
        return Ok(());
    };
    if steps == 0 {
        return Err(SceneValidationError::RangeViolation {
            part: "terrain lod policy steps",
            minimum: 1,
            maximum: u64::from(u8::MAX),
            observed: 0,
        });
    }
    let maximum =
        max_uniform_steps(columns, rows).ok_or(SceneValidationError::ArithmeticOverflow {
            part: "terrain lod policy",
        })?;
    if u64::from(steps) > maximum {
        return Err(SceneValidationError::RangeViolation {
            part: "terrain lod policy steps",
            minimum: 1,
            maximum,
            observed: u64::from(steps),
        });
    }
    Ok(())
}

/// Whole halving steps a positive extent survives before reaching one sample.
///
/// Returns `None` for an empty extent, which carries no interval to halve.
fn reducible_steps(extent_samples: u32) -> Option<u64> {
    let intervals = u64::from(extent_samples).checked_sub(1)?;
    Some(u64::from(u64::BITS) - u64::from(intervals.leading_zeros()))
}

/// Whole halving steps a grid of the given extents survives on both axes.
///
/// This is the exact bound [`TerrainLodPolicy::UniformSteps`] must respect, so
/// governed authoring can reject an over-large `steps` when its document is
/// read rather than leaving it to fail against the composed grid.
pub(crate) fn max_uniform_steps(columns: u32, rows: u32) -> Option<u64> {
    Some(reducible_steps(columns)?.min(reducible_steps(rows)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral_surface(material: u32) -> SceneTerrainSurface {
        SceneTerrainSurface::new(
            MaterialHandle::from_raw(material),
            SceneTerrainAppearance::NEUTRAL,
        )
    }

    #[test]
    fn one_material_backs_several_surfaces_that_differ_in_appearance() {
        // Two source indices binding one material and differing only in
        // appearance must remain two surfaces; collapsing them here would
        // erase the only thing that tells them apart.
        let tinted = SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance {
                tint_percent: [83, 108, 54],
                ..SceneTerrainAppearance::NEUTRAL
            },
        );
        let plain = neutral_surface(0);
        assert_eq!(tinted.material(), plain.material());
        assert_ne!(tinted, plain);

        let terrain = SceneTerrain::new(SceneTerrainParts {
            columns: 2,
            rows: 2,
            origin_east_millimeters: -600,
            origin_north_millimeters: -600,
            sample_spacing_millimeters: 600,
            heights_millimeters: vec![0; 4],
            surface_indices: vec![0, 1, 1, 0],
            surfaces: vec![plain, tinted],
            lod_policy: TerrainLodPolicy::FullResolution,
        })
        .expect("two surfaces over one material are addressable");
        assert_eq!(terrain.surfaces().len(), 2);
        assert_eq!(
            terrain.surfaces()[1].appearance().tint_percent,
            [83, 108, 54]
        );
    }

    fn terrain(
        heights: Vec<i32>,
        indices: Vec<u8>,
        policy: TerrainLodPolicy,
    ) -> Result<SceneTerrain, SceneValidationError> {
        SceneTerrain::new(SceneTerrainParts {
            columns: 2,
            rows: 2,
            origin_east_millimeters: -600,
            origin_north_millimeters: -600,
            sample_spacing_millimeters: 600,
            heights_millimeters: heights,
            surface_indices: indices,
            surfaces: vec![neutral_surface(0)],
            lod_policy: policy,
        })
    }

    #[test]
    fn grids_must_carry_exactly_columns_times_rows_samples() {
        assert!(terrain(vec![0; 4], vec![0; 4], TerrainLodPolicy::FullResolution).is_ok());
        assert!(matches!(
            terrain(vec![0; 3], vec![0; 4], TerrainLodPolicy::FullResolution),
            Err(SceneValidationError::TerrainSampleCount {
                expected: 4,
                observed: 3,
                ..
            })
        ));
    }

    #[test]
    fn surface_indices_must_address_the_surface_list() {
        assert!(matches!(
            terrain(
                vec![0; 4],
                vec![0, 0, 0, 1],
                TerrainLodPolicy::FullResolution
            ),
            Err(SceneValidationError::MaterialIndexOutOfRange {
                index: 1,
                limit: 1,
                ..
            })
        ));
    }

    #[test]
    fn uniform_steps_are_bounded_by_the_grid() {
        let policy = TerrainLodPolicy::UniformSteps { steps: 1 };
        assert!(terrain(vec![0; 4], vec![0; 4], policy).is_ok());
        assert!(matches!(
            terrain(
                vec![0; 4],
                vec![0; 4],
                TerrainLodPolicy::UniformSteps { steps: 0 }
            ),
            Err(SceneValidationError::RangeViolation { observed: 0, .. })
        ));
        assert!(matches!(
            terrain(
                vec![0; 4],
                vec![0; 4],
                TerrainLodPolicy::UniformSteps { steps: 2 }
            ),
            Err(SceneValidationError::RangeViolation {
                maximum: 1,
                observed: 2,
                ..
            })
        ));
    }

    #[test]
    fn dimensions_and_spacing_fail_closed() {
        assert!(matches!(
            SceneTerrain::new(SceneTerrainParts {
                columns: 1,
                rows: 2,
                origin_east_millimeters: 0,
                origin_north_millimeters: 0,
                sample_spacing_millimeters: 600,
                heights_millimeters: vec![0; 2],
                surface_indices: vec![0; 2],
                surfaces: vec![neutral_surface(0)],
                lod_policy: TerrainLodPolicy::FullResolution,
            }),
            Err(SceneValidationError::InvalidDimensions { width: 1, .. })
        ));
        assert!(matches!(
            SceneTerrain::new(SceneTerrainParts {
                columns: 2,
                rows: 2,
                origin_east_millimeters: 0,
                origin_north_millimeters: 0,
                sample_spacing_millimeters: 0,
                heights_millimeters: vec![0; 4],
                surface_indices: vec![0; 4],
                surfaces: vec![neutral_surface(0)],
                lod_policy: TerrainLodPolicy::FullResolution,
            }),
            Err(SceneValidationError::ExtentNotPositive { .. })
        ));
    }
}
