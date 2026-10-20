//! Authored generation profiles: the validated content recipe for generated worlds.
//!
//! A [`GenerationProfile`] carries every content choice the generator used to compile:
//! biome definitions and thresholds, terrain material palette, prop placement
//! probabilities with asset selections and scales, and the catalog environment
//! references. Profiles load from authored JSON assets ([`GenerationProfile::from_authoring`]);
//! there is no compiled-in recipe, so profiles vary content while the executable stays
//! unchanged.

use worldrunner_asset_id::{LogicalAssetId, QualityTier};

/// Failure modes for loading an authored [`GenerationProfile`] asset.
#[derive(Debug)]
pub enum ProfileAssetError {
    /// The asset file could not be read.
    Io {
        /// Asset path that failed.
        path: std::path::PathBuf,
        /// Underlying filesystem error.
        error: std::io::Error,
    },
    /// The asset did not parse as a generation profile.
    Parse {
        /// Asset path that failed.
        path: std::path::PathBuf,
        /// Underlying deserialization error.
        error: serde_json::Error,
    },
    /// The asset parsed but violated a profile invariant.
    Invalid {
        /// Asset path that failed.
        path: std::path::PathBuf,
        /// Validation message.
        message: String,
    },
}

impl std::fmt::Display for ProfileAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => {
                write!(
                    f,
                    "cannot read generation profile {}: {error}",
                    path.display()
                )
            }
            Self::Parse { path, error } => {
                write!(
                    f,
                    "generation profile {} is malformed: {error}",
                    path.display()
                )
            }
            Self::Invalid { path, message } => {
                write!(
                    f,
                    "generation profile {} is invalid: {message}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ProfileAssetError {}

/// Domain separator for the profile content digest.
const PROFILE_DIGEST_DOMAIN: &[u8] = b"REAL-DISUNITY-GENERATION-PROFILE-DIGEST-V1";

/// Authored content recipe for one generator run.
///
/// Every field participates in [`GenerationProfile::digest`], so any edit changes
/// the recorded generation identity while the user's seed stays untouched.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationProfile {
    /// Complete named inputs, separate from the content-profile digest.
    pub requests: Vec<crate::GenerationPreset>,
    /// Stable label used in provenance records and digest input.
    pub name: String,
    /// Provenance citation for humans; deliberately outside the digest input.
    pub source: String,
    /// Biome definitions and the classification thresholds that select them.
    pub biomes: BiomeProfile,
    /// Terrain material palette and the surface classification thresholds.
    pub surfaces: SurfaceProfile,
    /// Prop placement probabilities, asset selections, and scales.
    pub placement: PlacementProfile,
    /// Catalog environment references published with the generated world.
    pub catalog: CatalogProfile,
}

/// Biome definitions plus the thresholds that route a region sample to one of them.
///
/// The three roles (coastal, temperate, highland) are generator mechanics; the
/// definition ids and biome asset ids are authored content.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeProfile {
    /// Biome selected for samples near or below sea level.
    pub coastal: BiomeChoice,
    /// Biome selected for ordinary land samples.
    pub temperate: BiomeChoice,
    /// Biome selected for cold or high samples.
    pub highland: BiomeChoice,
    /// Coastal band height above sea level in millimeters.
    pub coastal_max_height_above_sea_millimeters: i64,
    /// Samples at or below this temperature classify as highland.
    pub highland_max_temperature: u16,
    /// Samples above this elevation in millimeters classify as highland.
    pub highland_min_height_millimeters: i64,
}

/// One authored biome definition entry.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeChoice {
    /// Definition id referenced by generated biome regions.
    pub definition_id: u32,
    /// Logical biome asset id, resolved during validation.
    pub biome_asset_id: String,
}

/// Terrain material palette plus surface classification thresholds.
///
/// Palette positions are mechanics: index 0 is the water surface, index 1 the
/// ordinary ground, index 2 the rock/dry surface.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceProfile {
    /// Exactly three material slots in mechanics order (water, ground, rock).
    pub materials: [MaterialChoice; 3],
    /// Moisture samples strictly below this value classify as dry (rock).
    pub dry_moisture_max: u16,
}

/// One authored terrain material slot.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialChoice {
    /// Logical material asset id, resolved during validation.
    pub material_asset_id: String,
    /// Terrain UV scale in millimeters.
    pub uv_scale_millimeters: u32,
}

/// Prop placement probabilities, asset selections, and scale ranges.
///
/// Hash draws are generator mechanics (fixed byte slices of the per-candidate
/// stream); the denominators applied to those draws and the resulting asset and
/// scale choices are authored content.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementProfile {
    /// One in N candidates on rock terrain places a rock prop.
    pub rock_denominator_rocky_terrain: u64,
    /// One in N candidates on other terrain places a rock prop.
    pub rock_denominator_other_terrain: u64,
    /// One in N remaining candidates on rock terrain places grass.
    pub grass_denominator_rocky_terrain: u64,
    /// One in N grass candidates on other terrain upgrades to a tree.
    pub tree_denominator: u64,
    /// One in N rock props uses the first rock asset instead of the second.
    pub rock_split_denominator: u64,
    /// First rock asset choice.
    pub rock_primary: PropFamily,
    /// Second rock asset choice.
    pub rock_secondary: PropFamily,
    /// Tree asset choice.
    pub tree: PropFamily,
    /// Grass asset choice.
    pub grass: PropFamily,
}

/// One authored prop family: logical asset id plus its scale range.
///
/// A candidate's scale draw is `base_scale_millionths + draw % scale_jitter_modulus`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropFamily {
    /// Logical environment asset id, resolved during validation.
    pub asset_id: String,
    /// Base uniform scale in millionths.
    pub base_scale_millionths: u32,
    /// Inclusive jitter range added to the base scale; must be at least one.
    pub scale_jitter_modulus: u32,
}

/// Catalog environment references published in the generated catalog binding.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogProfile {
    /// Preferred art sets in priority order.
    pub preferred_art_sets: Vec<String>,
    /// Preferred platform profiles in priority order.
    pub preferred_platform_profiles: Vec<String>,
    /// Maximum quality tier the generated world admits.
    pub maximum_quality: QualityTier,
}

impl GenerationProfile {
    /// Loads an authored profile asset; the only construction path.
    ///
    /// Missing or malformed assets are errors, never defaults: generation
    /// content must be authored, so a broken asset fails before any world
    /// is produced.
    pub fn from_authoring(path: &std::path::Path) -> Result<Self, ProfileAssetError> {
        let bytes = std::fs::read(path).map_err(|error| ProfileAssetError::Io {
            path: path.to_owned(),
            error,
        })?;
        let profile: Self =
            serde_json::from_slice(&bytes).map_err(|error| ProfileAssetError::Parse {
                path: path.to_owned(),
                error,
            })?;
        profile
            .validate()
            .map_err(|message| ProfileAssetError::Invalid {
                path: path.to_owned(),
                message,
            })?;
        Ok(profile)
    }

    /// Validates the profile so every selected logical id resolves before
    /// generation can publish a world.
    pub fn validate(&self) -> Result<(), String> {
        validate_non_empty(&self.name, "profile name")?;
        crate::request::validate_presets(&self.requests)?;
        validate_biomes(&self.biomes)?;
        validate_surfaces(&self.surfaces)?;
        validate_placement(&self.placement)?;
        validate_catalog(&self.catalog)?;
        Ok(())
    }

    /// Computes the canonical content digest of this profile.
    ///
    /// Two profiles with equal content produce equal digests; any authored edit
    /// changes the digest. The digest is independent of the world seed.
    pub fn digest(&self) -> ProfileDigest {
        let mut hasher = blake3::Hasher::new();
        hasher.update(PROFILE_DIGEST_DOMAIN);
        hash_string(&mut hasher, &self.name);
        hash_string(&mut hasher, &self.biomes.coastal.biome_asset_id);
        hash_string(&mut hasher, &self.biomes.temperate.biome_asset_id);
        hash_string(&mut hasher, &self.biomes.highland.biome_asset_id);
        hasher.update(&self.biomes.coastal.definition_id.to_le_bytes());
        hasher.update(&self.biomes.temperate.definition_id.to_le_bytes());
        hasher.update(&self.biomes.highland.definition_id.to_le_bytes());
        hasher.update(
            &self
                .biomes
                .coastal_max_height_above_sea_millimeters
                .to_le_bytes(),
        );
        hasher.update(&self.biomes.highland_max_temperature.to_le_bytes());
        hasher.update(&self.biomes.highland_min_height_millimeters.to_le_bytes());
        for material in &self.surfaces.materials {
            hash_string(&mut hasher, &material.material_asset_id);
            hasher.update(&material.uv_scale_millimeters.to_le_bytes());
        }
        hasher.update(&self.surfaces.dry_moisture_max.to_le_bytes());
        hasher.update(&self.placement.rock_denominator_rocky_terrain.to_le_bytes());
        hasher.update(&self.placement.rock_denominator_other_terrain.to_le_bytes());
        hasher.update(&self.placement.grass_denominator_rocky_terrain.to_le_bytes());
        hasher.update(&self.placement.tree_denominator.to_le_bytes());
        hasher.update(&self.placement.rock_split_denominator.to_le_bytes());
        for family in [
            &self.placement.rock_primary,
            &self.placement.rock_secondary,
            &self.placement.tree,
            &self.placement.grass,
        ] {
            hash_string(&mut hasher, &family.asset_id);
            hasher.update(&family.base_scale_millionths.to_le_bytes());
            hasher.update(&family.scale_jitter_modulus.to_le_bytes());
        }
        hasher.update(
            &u64::try_from(self.catalog.preferred_art_sets.len())
                .expect("art set count fits in u64")
                .to_le_bytes(),
        );
        for art_set in &self.catalog.preferred_art_sets {
            hash_string(&mut hasher, art_set);
        }
        hasher.update(
            &u64::try_from(self.catalog.preferred_platform_profiles.len())
                .expect("platform profile count fits in u64")
                .to_le_bytes(),
        );
        for platform_profile in &self.catalog.preferred_platform_profiles {
            hash_string(&mut hasher, platform_profile);
        }
        hasher.update(match self.catalog.maximum_quality {
            QualityTier::Baseline => &[0_u8],
            QualityTier::High => &[1_u8],
            QualityTier::Ultra => &[2_u8],
        });
        ProfileDigest(*hasher.finalize().as_bytes())
    }
}

/// Canonical content digest of a [`GenerationProfile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProfileDigest([u8; 32]);

impl ProfileDigest {
    /// Lowercase hexadecimal form for provenance records.
    pub fn to_hex(self) -> String {
        self.0
            .iter()
            .fold(String::with_capacity(64), |mut output, byte| {
                use std::fmt::Write as _;

                write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
                output
            })
    }
}

fn validate_biomes(biomes: &BiomeProfile) -> Result<(), String> {
    validate_logical_id(&biomes.coastal.biome_asset_id, "coastal biome asset id")?;
    validate_logical_id(&biomes.temperate.biome_asset_id, "temperate biome asset id")?;
    validate_logical_id(&biomes.highland.biome_asset_id, "highland biome asset id")?;
    if biomes.coastal_max_height_above_sea_millimeters < 0 {
        return Err("coastal band height must not be negative".to_owned());
    }
    if biomes.highland_max_temperature == 0 {
        return Err("highland temperature threshold must be at least one".to_owned());
    }
    if biomes.highland_min_height_millimeters < 0 {
        return Err("highland elevation threshold must not be negative".to_owned());
    }
    let definitions = [
        biomes.coastal.definition_id,
        biomes.temperate.definition_id,
        biomes.highland.definition_id,
    ];
    for (position, definition) in definitions.iter().enumerate() {
        if definitions[..position].contains(definition) {
            return Err(format!("duplicate biome definition id {definition}"));
        }
    }
    Ok(())
}

fn validate_surfaces(surfaces: &SurfaceProfile) -> Result<(), String> {
    for (position, material) in surfaces.materials.iter().enumerate() {
        validate_logical_id(
            &material.material_asset_id,
            &format!("terrain material slot {position} asset id"),
        )?;
        if material.uv_scale_millimeters == 0 {
            return Err(format!(
                "terrain material slot {position} uv scale must be positive"
            ));
        }
    }
    if surfaces.dry_moisture_max == 0 {
        return Err("dry moisture threshold must be at least one".to_owned());
    }
    Ok(())
}

fn validate_placement(placement: &PlacementProfile) -> Result<(), String> {
    for (label, denominator) in [
        (
            "rock denominator on rock terrain",
            placement.rock_denominator_rocky_terrain,
        ),
        (
            "rock denominator on other terrain",
            placement.rock_denominator_other_terrain,
        ),
        (
            "grass denominator on rock terrain",
            placement.grass_denominator_rocky_terrain,
        ),
        ("tree denominator", placement.tree_denominator),
        ("rock split denominator", placement.rock_split_denominator),
    ] {
        if denominator == 0 {
            return Err(format!("{label} must be at least one"));
        }
    }
    for (label, family) in [
        ("primary rock", &placement.rock_primary),
        ("secondary rock", &placement.rock_secondary),
        ("tree", &placement.tree),
        ("grass", &placement.grass),
    ] {
        validate_logical_id(&family.asset_id, &format!("{label} prop asset id"))?;
        if family.base_scale_millionths == 0 {
            return Err(format!("{label} base scale must be positive"));
        }
        if family.scale_jitter_modulus == 0 {
            return Err(format!("{label} scale jitter modulus must be at least one"));
        }
    }
    Ok(())
}

fn validate_catalog(catalog: &CatalogProfile) -> Result<(), String> {
    for art_set in &catalog.preferred_art_sets {
        validate_non_empty(art_set, "preferred art set")?;
    }
    for platform_profile in &catalog.preferred_platform_profiles {
        validate_non_empty(platform_profile, "preferred platform profile")?;
    }
    Ok(())
}

fn validate_non_empty(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    Ok(())
}

fn validate_logical_id(value: &str, label: &str) -> Result<(), String> {
    LogicalAssetId::new(value)
        .map(|_| ())
        .map_err(|error| format!("{label} {value:?} does not resolve: {error}"))
}

fn hash_string(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(
        &u64::try_from(value.len())
            .expect("string length fits in u64")
            .to_le_bytes(),
    );
    hasher.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authored() -> GenerationProfile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generation-profile-v1.json");
        GenerationProfile::from_authoring(&path).expect("authored generation profile parses")
    }

    #[test]
    fn authored_profile_validates() {
        assert_eq!(authored().validate(), Ok(()));
    }

    #[test]
    fn synthetic_profile_has_a_stable_digest() {
        // Fresh neutral namespace and original synthetic recipe define this vector.
        assert_eq!(
            authored().digest().to_hex(),
            "e18387be2f0539d9a6468ada5376fd87c683ead7940ff7e7d03e96dae7310ace"
        );
    }

    #[test]
    fn from_authoring_rejects_missing_files_without_defaults() {
        let error = GenerationProfile::from_authoring(std::path::Path::new(
            "assets/authoring/does-not-exist.json",
        ))
        .unwrap_err();
        assert!(matches!(error, ProfileAssetError::Io { .. }), "{error}");
    }

    #[test]
    fn from_authoring_rejects_malformed_assets() {
        let path = std::env::temp_dir().join("worldrunner-malformed-generation-profile.json");
        std::fs::write(&path, "{ not json").unwrap();
        let error = GenerationProfile::from_authoring(&path).unwrap_err();
        assert!(matches!(error, ProfileAssetError::Parse { .. }), "{error}");
    }

    #[test]
    fn from_authoring_rejects_invalid_content() {
        let mut profile = authored();
        profile.placement.grass.asset_id = "no-namespace-here".to_owned();
        let path = std::env::temp_dir().join("worldrunner-invalid-generation-profile.json");
        std::fs::write(&path, serde_json::to_string(&profile).unwrap()).unwrap();
        let error = GenerationProfile::from_authoring(&path).unwrap_err();
        assert!(
            matches!(error, ProfileAssetError::Invalid { .. }),
            "{error}"
        );
    }

    #[test]
    fn unresolvable_asset_id_fails_validation() {
        let mut profile = authored();
        profile.placement.grass.asset_id = "no-namespace-here".to_owned();
        let error = profile.validate().unwrap_err();
        assert!(error.contains("does not resolve"), "{error}");
    }

    #[test]
    fn zero_denominator_fails_validation() {
        let mut profile = authored();
        profile.placement.tree_denominator = 0;
        assert_eq!(
            profile.validate().unwrap_err(),
            "tree denominator must be at least one"
        );
    }

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let authored = authored();
        assert_eq!(authored.digest(), authored.digest());
        let mut renamed = authored.clone();
        renamed.name = "other".to_owned();
        assert_ne!(authored.digest(), renamed.digest());
        let mut threshold = authored.clone();
        threshold.biomes.highland_max_temperature += 1;
        assert_ne!(authored.digest(), threshold.digest());
    }

    #[test]
    fn digest_ignores_the_provenance_field() {
        let mut resourced = authored();
        resourced.source = "a different citation".to_owned();
        assert_eq!(authored().digest(), resourced.digest());
    }
}
