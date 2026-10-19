//! Authored environment recipe: vegetation level-of-detail and wind.
//!
//! LOD selection distances are strictly increasing so exactly one level
//! matches at any distance, and wind parameters are bounded unit-scoped
//! integers; no timing or amplitude is left implicit.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::numeric::MICRO_SCALE;
use crate::document::validate::{
    require_identifier, require_positive_u32, require_range, require_sorted_unique, require_uri,
};

/// One authored level-of-detail level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodLevel {
    /// Zero-based level index; level zero is the nearest representation.
    pub level_index: u8,
    /// Distance in millimeters up to which this level is selected.
    pub max_distance_millimeters: u64,
    /// Structural URI of the mesh asset this level renders.
    pub mesh_uri: String,
}

impl LodLevel {
    /// Returns the zero-based level index.
    #[must_use]
    pub const fn level_index(&self) -> u8 {
        self.level_index
    }

    /// Returns the selection distance ceiling in millimeters.
    #[must_use]
    pub const fn max_distance_millimeters(&self) -> u64 {
        self.max_distance_millimeters
    }

    /// Returns the structural URI of this level's mesh asset.
    #[must_use]
    pub fn mesh_uri(&self) -> &str {
        &self.mesh_uri
    }
}

/// One authored level-of-detail policy.
///
/// Levels must be authored in strictly ascending, contiguous, duplicate-free
/// `level_index` order with strictly increasing selection distances, so level
/// selection is deterministic and total.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LodPolicy {
    lod_id: String,
    levels: Vec<LodLevel>,
}

impl LodPolicy {
    /// Validates and adopts the policy identifier and ordered levels.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the identifier is malformed or
    /// the level list violates ordering, contiguity, or distance rules.
    pub fn new(lod_id: String, levels: Vec<LodLevel>) -> Result<Self, ContractDocumentError> {
        let policy = Self { lod_id, levels };
        policy.validate()?;
        Ok(policy)
    }

    /// Returns the policy identifier.
    #[must_use]
    pub fn lod_id(&self) -> &str {
        &self.lod_id
    }

    /// Returns the strictly ordered levels.
    #[must_use]
    pub fn levels(&self) -> &[LodLevel] {
        &self.levels
    }

    /// Validates the identifier and level closure.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_identifier("lod policy lod_id", &self.lod_id)?;
        if self.levels.is_empty() {
            return Err(ContractDocumentError::EmptyCollection {
                field: "lod policy levels",
            });
        }
        require_sorted_unique(
            "lod policy levels",
            self.levels.iter().map(|level| u64::from(level.level_index)),
        )?;
        for (position, level) in self.levels.iter().enumerate() {
            require_uri("lod level mesh URI", &level.mesh_uri)?;
            require_positive_distance(level)?;
            require_contiguous_index(level, position)?;
            enforce_distance_order(&self.levels, position)?;
        }
        Ok(())
    }
}

/// Requires one strictly positive selection distance.
fn require_positive_distance(level: &LodLevel) -> Result<(), ContractDocumentError> {
    require_range(
        "lod level max_distance_millimeters",
        1,
        i128::from(u64::MAX),
        i128::from(level.max_distance_millimeters),
    )
}

/// Requires level indices to be contiguous and zero-based.
fn require_contiguous_index(
    level: &LodLevel,
    position: usize,
) -> Result<(), ContractDocumentError> {
    let expected = u64::try_from(position).unwrap_or(u64::MAX);
    if u64::from(level.level_index) != expected {
        return Err(ContractDocumentError::ViolatedInvariant {
            field: "lod policy levels",
            reason: "level indices must be contiguous and start at zero",
        });
    }
    Ok(())
}

/// Requires each level's distance to exceed its predecessor's distance.
fn enforce_distance_order(
    levels: &[LodLevel],
    position: usize,
) -> Result<(), ContractDocumentError> {
    if position == 0 {
        return Ok(());
    }
    let previous = &levels[position - 1];
    let current = &levels[position];
    if current.max_distance_millimeters <= previous.max_distance_millimeters {
        return Err(ContractDocumentError::ViolatedInvariant {
            field: "lod level max_distance_millimeters",
            reason: "selection distances must strictly increase with level index",
        });
    }
    Ok(())
}

/// Authored wind parameters.
///
/// Direction is a signed planar angle; amplitude is a unit-interval bend
/// fraction; period is a strictly positive whole-millisecond duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindRecipe {
    /// Planar wind direction in microradians.
    pub direction_microradians: i64,
    /// Peak bend amplitude as a unit-fraction micro value (`0..=1_000_000`).
    pub amplitude_micro: u64,
    /// Full oscillation period in milliseconds; strictly positive.
    pub period_milliseconds: u32,
}

impl WindRecipe {
    /// Returns the planar wind direction in microradians.
    #[must_use]
    pub const fn direction_microradians(&self) -> i64 {
        self.direction_microradians
    }

    /// Returns the peak bend amplitude micro value.
    #[must_use]
    pub const fn amplitude_micro(&self) -> u64 {
        self.amplitude_micro
    }

    /// Returns the oscillation period in milliseconds.
    #[must_use]
    pub const fn period_milliseconds(&self) -> u32 {
        self.period_milliseconds
    }

    /// Validates amplitude and period bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the amplitude leaves the unit
    /// interval or the period is zero.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range(
            "wind amplitude_micro",
            0,
            i128::from(MICRO_SCALE),
            i128::from(self.amplitude_micro),
        )?;
        require_positive_u32("wind period_milliseconds", self.period_milliseconds)
    }
}

/// Authored environment recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRecipe {
    lod_policies: Vec<LodPolicy>,
    wind: WindRecipe,
}

impl EnvironmentRecipe {
    /// Validates and adopts the ordered LOD policies and wind parameters.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when any child part is malformed.
    pub fn new(
        lod_policies: Vec<LodPolicy>,
        wind: WindRecipe,
    ) -> Result<Self, ContractDocumentError> {
        let environment = Self { lod_policies, wind };
        environment.validate()?;
        Ok(environment)
    }

    /// Returns the strictly ordered LOD policies.
    #[must_use]
    pub fn lod_policies(&self) -> &[LodPolicy] {
        &self.lod_policies
    }

    /// Returns the wind parameters.
    #[must_use]
    pub const fn wind(&self) -> &WindRecipe {
        &self.wind
    }

    /// Validates ordering, uniqueness, and every child part.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_sorted_unique(
            "environment lod_policies",
            self.lod_policies
                .iter()
                .map(|policy| policy.lod_id.as_str()),
        )?;
        for policy in &self.lod_policies {
            policy.validate()?;
        }
        self.wind.validate()
    }
}
