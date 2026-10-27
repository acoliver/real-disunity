//! GPU-free adaptation of independently verified normal publications into
//! immutable, wholly owned CPU render plans.
//!
//! Callers provide every exact document, component, capability, and payload
//! contract route. This crate provides no dataset routes, defaults, path-based
//! lookup, content sniffing, or renderer/GPU dependency. The capability
//! factories additionally mint fail-closed
//! [`worldrunner_world_runtime::CapabilityHandler`] implementations for wholly
//! owned exact bytes, canonical package, mesh, material, and texture
//! artifacts, and bounded pure U8 and little-endian U16 grids, each wholly
//! configured by the caller. Authored [`ContractDocument`] route
//! declarations activate into one exact [`RenderWorldContract`] through
//! caller-registered implementation factories keyed by exact handler factory
//! URI, with no fallback across route kinds. Every activated implementation
//! additionally carries an exact activation-bound scene factory: route
//! factories produce closed [`SceneContribution`] values and the document
//! factory composes one mandatory, wholly owned, pathless
//! [`RenderScene`] that the final
//! [`RenderWorldPlan`] retains.

mod adapter;
mod capability;
mod contract;
mod discovery;
mod document;
mod error;
mod implementation_registry;
mod limits;
mod plan;
mod plan_closure;
mod plan_scene;
mod scene;
pub mod world_schema;

pub use adapter::{CpuRenderPayload, RenderPayload, RenderPayloadAdapter};
pub use capability::{
    CapabilityConfigError, ElementPolicy, ExactBytes, ExactCapabilityRoute, GridShape,
    MaterialPayload, PackagePayload, StaticMeshPayload, TexturePayload, U8Grid, U16LeGrid,
    exact_bytes_handler, material_handler, package_handler, static_mesh_handler, texture_handler,
    u8_grid_handler, u16_le_grid_handler,
};
pub use contract::{ContractRegistration, RenderWorldContract, RenderWorldContractBuilder};
pub use discovery::{
    DiscoveredRenderContract, RENDER_CONTRACT_CAPABILITY_URI, RenderContractDiscoveryError,
    RenderContractDiscoveryLimits, discover_render_contract,
};
pub use error::{
    RenderAdapterError, RenderContractError, RenderImplementationError, RenderPlanError,
};
pub use implementation_registry::{
    ActivatedRenderWorldContract, CapabilityImplementation, CapabilityImplementationFactory,
    ComponentImplementation, ComponentImplementationFactory, DocumentImplementation,
    DocumentImplementationFactory, RenderFactoryError, RenderImplementationRegistry,
    RenderImplementationRegistryBuilder, RenderRouteKind, activate_render_world_contract,
};
pub use limits::RenderWorldLimits;
pub use plan::{
    RenderContribution, RenderWorldPlan, build_discovered_render_world_plan,
    build_render_world_plan,
};
pub use scene::{
    BillboardPolicy, DIRECTION_MICRO_SCALE, DocumentSceneInput, EFFECTS_MICRO_SCALE,
    MATERIAL_MICRO_SCALE, MaterialHandle, MeshHandle, ProvenSceneContribution,
    ROTATION_MICRO_SCALE, RenderScene, RouteDeclarationRef, RouteSceneFactory, RouteSceneInput,
    SceneAlphaMode, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading,
    SceneCompositionFactory, SceneContribution, SceneDirection, SceneEffects, SceneEffectsParts,
    SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject,
    SceneParts, ScenePlacement, ScenePostStage, SceneResources, SceneRotation, SceneShadowCascades,
    SceneSky, SceneSsao, SceneSsaoFilter, SceneSsaoKernel, SceneSsr, SceneSsrComposition,
    SceneSsrGridDomain, SceneSsrPassBehind, SceneSsrTraversal, SceneSun, SceneTerrain,
    SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture, SceneTransform,
    SceneValidationError, SceneVertex, SceneWaterBody, SceneWaterSurface, TerrainLodPolicy,
    TexelFormat, TextureColorSpace, TextureEncoding, TextureFilter, TextureHandle, TextureSampler,
    TextureWrap,
};
pub use scene::{
    TerrainPagePolicy, TerrainPageSettings, TerrainPageSettingsError, TerrainPageSettingsParts,
    TerrainStreamingPolicy,
};

pub use document::{
    AmbientLightMicro, AssetBinding, AssetResolutionPolicy, AxisDirection, BindingElementPolicy,
    CameraIntrinsics, CapabilityRouteDeclaration, ComponentRouteDeclaration, ContractDocument,
    ContractDocumentError, ContractDocumentHeader, ContractDocumentParts, ContractLimits,
    CoordinateBasis, DirectionalLightMicro, DocumentRouteDeclaration, EffectsRecipe,
    EffectsRecipeParts, EnvironmentRecipe, Handedness, LodLevel, LodPolicy, MICRO_SCALE,
    MeshComponent, MicroFixed, PositionMillimeters, RENDER_CONTRACT_SCHEMA,
    RENDER_CONTRACT_SCHEMA_VERSION, RotationQuaternion, RouteCardinality, RouteDeclarations,
    RouteIdentity, SceneDescription, ShadowCascade, ShadowCascadesRecipe, SkyRecipe, SsaoRecipe,
    StreamLimits, TerrainMaterialSlot, TerrainRecipe, TransformMillimeters, UniformScaleMillionths,
    WaterPlaneRecipe, WindRecipe,
};

pub use scene::{
    SceneAdaptiveExposure, SceneBloom, SceneBloomLevelSelection, SceneBloomRecipe,
    SceneBloomReduction, SceneBloomTerminal, SceneBloomTintPass, SceneBrightCurve,
    SceneBrightExtraction, SceneDisplayGrading, SceneExposureAdaptation, SceneFxaaKernel,
    SceneFxaaLuminance, SceneFxaaQuality, SceneGaussianKernelRule, SceneGaussianLevel,
    SceneHdrPost, ScenePercentileExposure, SceneToneCurveBright, SceneToneMap, SceneVignette,
};

pub use scene::{
    SceneAmbientOcclusion, SceneCard, SceneFadeBand, SceneFoliageWind, SceneImpostor,
    SceneImpostorLayout, SceneLayerBlend, SceneLayerMode, SceneMaterialFeatures,
    SceneNormalEncoding, SceneNormalY, SceneTerrainContact, SceneTint, SceneTranslucency,
    SceneTranslucencyParts, SceneWind, SceneWindBand, SceneWindPhase,
};

pub use scene::{SceneWaterOptics, SceneWaterSettings, SceneWaterShoreline, SceneWaterTint};

#[cfg(feature = "ui")]
pub mod ui;

#[cfg(feature = "simulation")]
pub mod simulation;

pub use scene::{
    SceneAtmosphere, SceneCelestialDisc, SceneCelestialMesh, SceneHeightFog, SceneScattering,
    SceneScatteringFormat, SceneScatteringModel,
};
pub use worldrunner_material::BuiltinMaterial;

pub use scene::{
    SceneDiffuseEnvironment, SceneRoughnessRemap, SceneSh9, SceneSpecularConvention,
    SceneSpecularOcclusion, SceneSpecularVisibility,
};

pub use scene::{SceneDepthAdoption, SceneDepthFitting, SceneDepthSplits};

pub use scene::{
    TerrainColorization, TerrainColorizationChannels, TerrainLayer, TerrainLayeredSource,
    TerrainSplatFilter, TerrainTypeGrid,
};

pub use scene::{ScenePlanarWindField, SceneStaticWindModifier};
