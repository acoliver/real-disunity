//! GPU-free, immutable, wholly owned CPU render scenes.
//!
//! [`RenderScene`] and its parts validate geometry, materials, textures, effects
//! and terrain without a renderer or GPU dependency. This crate provides no
//! dataset routes, defaults, path-based lookup or content sniffing.

pub mod picking;
pub use picking::{ObjectId, PickHit};
pub mod post;
mod scene;
pub mod world_schema;

pub use scene::SceneAppend;
pub use scene::TextureAnisotropy;
pub use scene::{
    BillboardPolicy, DEFAULT_TAA_CLIP_SCALE_MICRO, DIRECTION_MICRO_SCALE, EFFECTS_MICRO_SCALE,
    MATERIAL_MICRO_SCALE, MAXIMUM_SUN_IRRADIANCE_MICRO, MAXIMUM_TAA_CLIP_SCALE_MICRO,
    MINIMUM_TAA_CLIP_SCALE_MICRO, MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene,
    SceneAffineMatrix, SceneAlphaMode, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor,
    SceneColorGrading, SceneDirection, SceneDownsampleRounding, SceneEffects, SceneEffectsParts,
    SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject,
    SceneOpaqueExperiment, SceneOpaqueFamily, SceneOpaqueMode, SceneOpaquePath, SceneOpaqueStorage,
    SceneOpaqueVisibility, SceneParts, ScenePostStage, SceneResources, SceneRotation,
    SceneShadowCascades, SceneShadowFilter, SceneShadowSampling, SceneSkeleton, SceneSkinInfluence,
    SceneSkinPose, SceneSky, SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel,
    SceneSsaoReceiver, SceneSsr, SceneSsrComposition, SceneSsrGridDomain, SceneSsrHitFacing,
    SceneSsrLodMapping, SceneSsrPassBehind, SceneSsrPyramid, SceneSsrTraversal, SceneSun,
    SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture,
    SceneTransform, SceneValidationError, SceneVertex, SceneWaterBody, SceneWaterSurface,
    SkeletonHandle, SkinPoseHandle, TerrainLodPolicy, TexelFormat, TextureColorSpace,
    TextureEncoding, TextureFilter, TextureHandle, TextureSampler, TextureWrap,
};
pub use scene::{SceneContourBlend, SceneContourFill, SceneContourStyle, SceneContours};
pub use scene::{
    SceneCubeDescriptor, SceneCubeFace, SceneDdsDescriptor, SceneDdsFormat, SceneTexelDescriptor,
    SceneTextureDescriptor,
};
pub use scene::{SceneMaterialChannels, TextureChannel};
pub use scene::{
    TerrainPagePolicy, TerrainPageSettings, TerrainPageSettingsError, TerrainPageSettingsParts,
    TerrainStreamingPolicy,
};

pub use scene::{SceneSmaaEdgeDetection, SceneSmaaPreset, SceneSmaaSettings};

pub use scene::{
    SceneAdaptiveExposure, SceneBloom, SceneBloomLevelSelection, SceneBloomRecipe,
    SceneBloomReduction, SceneBloomTerminal, SceneBloomTintPass, SceneBrightCurve,
    SceneBrightExtraction, SceneDisplayGrading, SceneExposureAdaptation, SceneFxaaKernel,
    SceneFxaaLuminance, SceneFxaaQuality, SceneGaussianKernelRule, SceneGaussianLevel,
    SceneHdrPost, ScenePercentileExposure, SceneToneCurveBright, SceneToneMap, SceneVignette,
};

pub use scene::{
    SceneAmbientOcclusion, SceneAtlasSampling, SceneCard, SceneFadeBand, SceneFadeCoverage,
    SceneFadeDirection, SceneFadeQuantization, SceneFadeReference, SceneFoldSampling,
    SceneFoliageWind, SceneImpostor, SceneImpostorChannels, SceneImpostorLayout, SceneLayerBlend,
    SceneLayerMode, SceneMaterialFeatures, SceneNormalEncoding, SceneNormalY, SceneTerrainContact,
    SceneTint, SceneTranslucency, SceneTranslucencyParts, SceneWind, SceneWindArc, SceneWindBand,
    SceneWindPhase, threshold_texels,
};

pub use scene::{
    SceneWaterEnvironment, SceneWaterFlowCoordinates, SceneWaterOptics, SceneWaterSeedCoordinates,
    SceneWaterSeedSampling, SceneWaterSettings, SceneWaterShadowFilter, SceneWaterShadowOffset,
    SceneWaterShadowResponse, SceneWaterShoreline, SceneWaterSunDisk, SceneWaterTint,
    SceneWaterWind, SceneWaterZeroFrequency,
};

#[cfg(feature = "simulation")]
pub mod simulation;

pub use scene::{
    SceneAtmosphere, SceneBackgroundDepthPolicy, SceneCelestialComposition, SceneCelestialDisc,
    SceneCelestialLighting, SceneCelestialMesh, SceneFogDensity, SceneFogDensityPolicy,
    SceneHeightFog, SceneScattering, SceneScatteringFormat, SceneScatteringModel,
    SceneSpecularCube,
};
pub use worldrunner_material::BuiltinMaterial;

pub use scene::{
    SceneDiffuseEnvironment, SceneDirectDiffuse, SceneRoughnessRemap, SceneSh9,
    SceneSpecularConvention, SceneSpecularOcclusion, SceneSpecularVisibility, SceneSurfaceDisk,
};

pub use scene::{
    SceneDepthAdoption, SceneDepthFitting, SceneDepthMinimumSpan, SceneDepthSplits,
    SceneLodMaterialSlot,
};

pub use scene::{
    TerrainColorization, TerrainColorizationChannels, TerrainLayer, TerrainLayeredSource,
    TerrainSplatFilter, TerrainTypeGrid,
};

pub use scene::{ScenePlanarWindField, SceneStaticWindModifier};

pub use scene::{TerrainLayerChannel, TerrainLayerChannelMap, TerrainLayerTexture};

pub use scene::{
    SceneDecal, SceneDecalAlphaMode, SceneDecalAngle, SceneDecalAxis, SceneDecalColorModifier,
    SceneDecalMapping, SceneDecalNormalFrame, SceneDecalReceivers, SceneDecalWriteMask,
};

pub use scene::{
    SceneEnvironment, SceneEnvironmentConsumers, SceneSpecularCubeFormat, SceneSpecularPreparation,
};
