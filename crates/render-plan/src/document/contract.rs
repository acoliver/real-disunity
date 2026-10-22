//! Versioned authored render documents with exact routes and validated recipes.
//!
//! Construction and decoding share the same validation closure. Runtime bindings
//! remain the authority for content digests, byte lengths, and capability routing.

use serde::{Deserialize, Serialize};

use crate::document::assets::{AssetBinding, AssetResolutionPolicy};
use crate::document::effects::EffectsRecipe;
use crate::document::environment::EnvironmentRecipe;
use crate::document::error::ContractDocumentError;
use crate::document::limits::ContractLimits;
use crate::document::routes::RouteDeclarations;
use crate::document::scene::SceneDescription;
use crate::document::spatial::CoordinateBasis;
use crate::document::terrain::TerrainRecipe;
use crate::document::validate::require_identifier;

/// Schema tag of the authored render-contract document family.
pub const RENDER_CONTRACT_SCHEMA: &str = "real-disunity.render-contract.v2";
/// Schema version of the authored render-contract document family.
pub const RENDER_CONTRACT_SCHEMA_VERSION: u32 = 2;

/// Version envelope shared by every authored render contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDocumentHeader {
    /// Schema family tag.
    pub schema: String,
    /// Schema family version.
    pub schema_version: u32,
}

impl ContractDocumentHeader {
    /// Verifies the envelope against the one exact known family.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] for any other tag or version.
    pub fn verify(&self) -> Result<(), ContractDocumentError> {
        if self.schema != RENDER_CONTRACT_SCHEMA {
            return Err(ContractDocumentError::SchemaTagMismatch {
                expected: RENDER_CONTRACT_SCHEMA,
                observed: self.schema.clone(),
            });
        }
        if self.schema_version != RENDER_CONTRACT_SCHEMA_VERSION {
            return Err(ContractDocumentError::UnsupportedSchemaVersion {
                expected: RENDER_CONTRACT_SCHEMA_VERSION,
                observed: self.schema_version,
            });
        }
        Ok(())
    }
}

/// Authored parts adopted by [`ContractDocument::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractDocumentParts {
    /// Coordinate and unit conventions.
    pub basis: CoordinateBasis,
    /// Runtime scene state.
    pub scene: SceneDescription,
    /// Terrain construction recipe.
    pub terrain: TerrainRecipe,
    /// Environment placement and motion recipe.
    pub environment: EnvironmentRecipe,
    /// Lighting, water, atmosphere, and camera recipe.
    pub effects: EffectsRecipe,
    /// Route declarations served by the contract.
    pub routes: RouteDeclarations,
    /// Exact asset-to-binding resolution policy.
    pub assets: AssetResolutionPolicy,
    /// Explicit operational limits.
    pub limits: ContractLimits,
}

/// Complete authored render contract.
///
/// Authored recipes bind to exact asset references. Runtime resource bindings
/// independently govern capability routing, digests, and byte lengths.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ContractDocument {
    #[serde(flatten)]
    header: ContractDocumentHeader,
    contract_id: String,
    basis: CoordinateBasis,
    scene: SceneDescription,
    terrain: TerrainRecipe,
    environment: EnvironmentRecipe,
    effects: EffectsRecipe,
    routes: RouteDeclarations,
    assets: AssetResolutionPolicy,
    limits: ContractLimits,
}

impl ContractDocument {
    /// Validates and adopts every authored contract part.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated authored
    /// invariant, including the cross-part closure rules.
    pub fn new(
        contract_id: String,
        parts: ContractDocumentParts,
    ) -> Result<Self, ContractDocumentError> {
        let document = Self {
            header: ContractDocumentHeader {
                schema: RENDER_CONTRACT_SCHEMA.to_owned(),
                schema_version: RENDER_CONTRACT_SCHEMA_VERSION,
            },
            contract_id,
            basis: parts.basis,
            scene: parts.scene,
            terrain: parts.terrain,
            environment: parts.environment,
            effects: parts.effects,
            routes: parts.routes,
            assets: parts.assets,
            limits: parts.limits,
        };
        document.validate()?;
        Ok(document)
    }

    /// Returns the version envelope.
    #[must_use]
    pub const fn header(&self) -> &ContractDocumentHeader {
        &self.header
    }

    /// Returns the authored contract identifier.
    #[must_use]
    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    /// Returns the authored coordinate basis.
    #[must_use]
    pub const fn basis(&self) -> &CoordinateBasis {
        &self.basis
    }

    /// Returns the authored scene description.
    #[must_use]
    pub const fn scene(&self) -> &SceneDescription {
        &self.scene
    }

    /// Returns the authored terrain recipe.
    #[must_use]
    pub const fn terrain(&self) -> &TerrainRecipe {
        &self.terrain
    }

    /// Returns the authored environment recipe.
    #[must_use]
    pub const fn environment(&self) -> &EnvironmentRecipe {
        &self.environment
    }

    /// Returns the authored effects recipe.
    #[must_use]
    pub const fn effects(&self) -> &EffectsRecipe {
        &self.effects
    }

    /// Returns the authored route declarations.
    #[must_use]
    pub const fn routes(&self) -> &RouteDeclarations {
        &self.routes
    }

    /// Returns the authored asset resolution policy.
    #[must_use]
    pub const fn assets(&self) -> &AssetResolutionPolicy {
        &self.assets
    }

    /// Returns the explicit authored limits.
    #[must_use]
    pub const fn limits(&self) -> &ContractLimits {
        &self.limits
    }

    /// Re-runs every authored invariant, including cross-part closure.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        self.header.verify()?;
        require_identifier("contract_id", &self.contract_id)?;
        self.basis.validate()?;
        self.scene.validate()?;
        self.terrain.validate()?;
        self.environment.validate()?;
        self.effects.validate()?;
        self.routes.validate()?;
        self.assets.validate()?;
        self.limits.validate()?;
        self.validate_limits_enforcement()?;
        self.validate_asset_closure()?;
        self.validate_route_closure()
    }

    /// Requires authored content counts to respect the authored limits.
    ///
    /// The stream bounds stay authored operational bounds for later runtime
    /// planning; this layer no longer sums or enforces binding byte lengths,
    /// because asset bindings carry no byte authority.
    fn validate_limits_enforcement(&self) -> Result<(), ContractDocumentError> {
        enforce_limit(
            "scene meshes",
            self.scene.meshes().len(),
            self.limits.max_mesh_components(),
        )?;
        for policy in self.environment.lod_policies() {
            enforce_limit(
                "lod policy levels",
                policy.levels().len(),
                self.limits.max_lod_levels_per_policy(),
            )?;
        }
        Ok(())
    }

    /// Requires every referenced asset URI to be declared by one binding.
    fn validate_asset_closure(&self) -> Result<(), ContractDocumentError> {
        for uri in self
            .scene
            .meshes()
            .iter()
            .map(crate::document::scene::MeshComponent::asset_uri)
        {
            require_resolved(self.assets.binding_for(uri), "scene meshes", uri)?;
        }
        require_resolved(
            self.assets.binding_for(self.terrain.height_asset_uri()),
            "terrain height",
            self.terrain.height_asset_uri(),
        )?;
        for slot in self.terrain.material_palette() {
            require_resolved(
                self.assets.binding_for(slot.material_uri()),
                "terrain material_palette",
                slot.material_uri(),
            )?;
        }
        self.validate_environment_asset_closure()?;
        require_resolved(
            self.assets.binding_for(self.effects.sky().asset_uri()),
            "effects sky",
            self.effects.sky().asset_uri(),
        )?;
        if let Some(water) = self.effects.water() {
            require_resolved(
                self.assets.binding_for(water.asset_uri()),
                "effects water",
                water.asset_uri(),
            )?;
        }
        Ok(())
    }

    /// Requires every LOD level mesh URI to be declared by one binding.
    fn validate_environment_asset_closure(&self) -> Result<(), ContractDocumentError> {
        for policy in self.environment.lod_policies() {
            for level in policy.levels() {
                require_resolved(
                    self.assets.binding_for(level.mesh_uri()),
                    "environment lod_policies",
                    level.mesh_uri(),
                )?;
            }
        }
        Ok(())
    }

    /// Requires capability routes whenever asset bindings are present.
    ///
    /// An empty capability list is valid at the pure route-schema level;
    /// authored contracts that bind assets must route them, so the presence
    /// of bindings closes the capability list over the contract.
    fn validate_route_closure(&self) -> Result<(), ContractDocumentError> {
        if self.assets.bindings().is_empty() {
            return Ok(());
        }
        if self.routes.capabilities().is_empty() {
            return Err(ContractDocumentError::ViolatedInvariant {
                field: "routes capabilities",
                reason: "capability routes must be declared when asset bindings are present",
            });
        }
        Ok(())
    }

    /// Serializes the validated document as deterministic compact JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError::Json`] when serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, ContractDocumentError> {
        serde_json::to_vec(self).map_err(ContractDocumentError::Json)
    }

    /// Decodes and fully validates one strict authored document.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] for malformed JSON, a wrong schema
    /// family or version, an unknown field, a missing mandatory value, or any
    /// violated authored invariant.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, ContractDocumentError> {
        let raw: RawContractDocument = serde_json::from_slice(bytes)?;
        ContractDocument::from_structural(raw)
    }

    fn from_structural(raw: RawContractDocument) -> Result<Self, ContractDocumentError> {
        let document = Self {
            header: ContractDocumentHeader {
                schema: raw.schema,
                schema_version: raw.schema_version,
            },
            contract_id: raw.contract_id,
            basis: raw.basis,
            scene: raw.scene,
            terrain: raw.terrain,
            environment: raw.environment,
            effects: raw.effects,
            routes: raw.routes,
            assets: raw.assets,
            limits: raw.limits,
        };
        document.validate()?;
        Ok(document)
    }
}

/// Requires one resolved binding for an asset URI reference.
fn require_resolved(
    binding: Option<&AssetBinding>,
    collection: &'static str,
    uri: &str,
) -> Result<(), ContractDocumentError> {
    if binding.is_some() {
        Ok(())
    } else {
        Err(ContractDocumentError::UnresolvedAssetUri {
            collection,
            uri: uri.to_owned(),
        })
    }
}

/// Requires one observed count to respect its authored maximum.
fn enforce_limit(
    field: &'static str,
    observed: usize,
    max: u32,
) -> Result<(), ContractDocumentError> {
    let observed =
        u64::try_from(observed).map_err(|_| ContractDocumentError::ArithmeticOverflow { field })?;
    if observed > u64::from(max) {
        return Err(ContractDocumentError::LimitExceeded {
            field,
            max: u64::from(max),
            observed,
        });
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContractDocument {
    schema: String,
    schema_version: u32,
    contract_id: String,
    basis: CoordinateBasis,
    scene: SceneDescription,
    terrain: TerrainRecipe,
    environment: EnvironmentRecipe,
    effects: EffectsRecipe,
    routes: RouteDeclarations,
    assets: AssetResolutionPolicy,
    limits: ContractLimits,
}

impl<'de> Deserialize<'de> for ContractDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawContractDocument::deserialize(deserializer)?;
        ContractDocument::from_structural(raw).map_err(serde::de::Error::custom)
    }
}
