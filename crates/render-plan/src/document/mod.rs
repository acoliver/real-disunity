//! Strict universal externally authored render-contract documents.
//!
//! This v1 document family is landscape recipe groundwork, not the complete
//! production route contract. The module defines the authored recipe schema
//! and its exact validation: deterministic integers only (whole millimeters,
//! micro-unit ratios, millidegree angles), `deny_unknown_fields` everywhere,
//! no serde defaults, keyed collections in strictly ascending unique order,
//! identifiers and URIs validated by the world-model universal validators,
//! and fail-closed handling of unknown schema versions, unknown fields, and
//! missing mandatory values. Cross-part closure requires every referenced
//! asset URI to be declared by exactly one asset binding, authored content
//! counts to respect the authored limits, and capability routes to be
//! declared whenever asset bindings are present; runtime capability routing
//! of bound content, digests, and byte lengths stay solely owned by the
//! resource-binding layer, and no rule confines scene content to the
//! terrain rectangle. The authored route declarations are data only: a
//! later registry must match their exact externally authored factory,
//! handler, and payload identities or fail closed. The coordinate basis is
//! external authority: generic processing must support every valid basis.

mod assets;
mod contract;
mod effects;
mod environment;
mod error;
mod limits;
mod numeric;
mod routes;
mod scene;
mod spatial;
mod terrain;
mod validate;

pub use assets::{AssetBinding, AssetResolutionPolicy};
pub use contract::{
    ContractDocument, ContractDocumentHeader, ContractDocumentParts, RENDER_CONTRACT_SCHEMA,
    RENDER_CONTRACT_SCHEMA_VERSION,
};
pub use effects::{
    AmbientLightMicro, CameraIntrinsics, DirectionalLightMicro, EffectsRecipe, EffectsRecipeParts,
    ShadowCascade, ShadowCascadesRecipe, SkyRecipe, SsaoRecipe, WaterPlaneRecipe,
};
pub use environment::{EnvironmentRecipe, LodLevel, LodPolicy, WindRecipe};
pub use error::ContractDocumentError;
pub use limits::{ContractLimits, StreamLimits};
pub use numeric::{MICRO_SCALE, MicroFixed};
pub use routes::{
    BindingElementPolicy, CapabilityRouteDeclaration, ComponentRouteDeclaration,
    DocumentRouteDeclaration, RouteCardinality, RouteDeclarations, RouteIdentity,
};
pub use scene::{MeshComponent, SceneDescription};
pub use spatial::{
    AxisDirection, CoordinateBasis, Handedness, PositionMillimeters, RotationQuaternion,
    TransformMillimeters, UniformScaleMillionths,
};
pub use terrain::{TerrainMaterialSlot, TerrainRecipe};
