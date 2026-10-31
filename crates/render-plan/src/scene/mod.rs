//! Closed owned renderer-domain scene model.
//!
//! A [`RenderScene`] is wholly owned CPU state: typed dense handles, owned
//! boxed payloads, and closed renderer-semantic variants. The module carries
//! no paths, no package, profile, or dataset identifiers, no role URI
//! interpretation, no `Default`, no fallback, and no data borrowed from a
//! publication. Every constructor validates its own closure, so an invalid
//! scene cannot exist.
mod decal;
pub use decal::{
    SceneDecal, SceneDecalAlphaMode, SceneDecalAngle, SceneDecalAxis, SceneDecalColorModifier,
    SceneDecalMapping, SceneDecalNormalFrame, SceneDecalReceivers, SceneDecalWriteMask,
};
mod atmosphere;
mod specular_cube;
mod specular_preparation;
pub use specular_cube::{SceneSpecularCube, SceneSpecularCubeFormat};
pub use specular_preparation::SceneSpecularPreparation;
mod celestial_lighting;
pub use celestial_lighting::SceneCelestialLighting;
mod fog_density;
pub use fog_density::SceneFogDensity;
mod scattering;
#[cfg(test)]
mod specular_preparation_tests;
pub use atmosphere::{
    SceneAtmosphere, SceneBackgroundDepthPolicy, SceneCelestialComposition, SceneCelestialDisc,
    SceneCelestialMesh, SceneFogDensityPolicy, SceneHeightFog,
};
pub use scattering::{SceneScattering, SceneScatteringFormat, SceneScatteringModel};
mod direction;
pub use direction::{DIRECTION_MICRO_SCALE, SceneDirection};
mod composition;
mod depth_fitting;
mod depth_footprint;
pub use depth_footprint::{SceneDownsampleRounding, SceneSsaoDepthFootprint};
mod effects;
mod environment;
pub use environment::{SceneEnvironment, SceneEnvironmentConsumers};
mod shadow_sampling;
pub use shadow_sampling::{SceneShadowFilter, SceneShadowSampling};
mod lighting;
mod obscurance;
pub use depth_fitting::{
    SceneDepthAdoption, SceneDepthFitting, SceneDepthMinimumSpan, SceneDepthSplits,
};
pub use lighting::{
    SceneDiffuseEnvironment, SceneDirectDiffuse, SceneRoughnessRemap, SceneSh9,
    SceneSpecularConvention, SceneSpecularOcclusion, SceneSpecularVisibility, SceneSurfaceDisk,
};
pub use obscurance::{SceneSsao, SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver};
mod water;
mod water_optics;
mod water_settings;
mod water_shadow;
pub use water_shadow::{SceneWaterShadowFilter, SceneWaterShadowOffset, SceneWaterShadowResponse};
mod water_terms;
pub use water::{SceneWaterBody, SceneWaterSurface};
pub use water_optics::{SceneWaterOptics, SceneWaterShoreline, SceneWaterTint};
pub use water_settings::SceneWaterSettings;
pub use water_terms::{
    SceneWaterEnvironment, SceneWaterFlowCoordinates, SceneWaterSeedCoordinates,
    SceneWaterSeedSampling, SceneWaterSunDisk, SceneWaterWind, SceneWaterZeroFrequency,
};
mod bloom;
mod exposure;
mod post;
pub use bloom::{
    SceneBloomLevelSelection, SceneBloomRecipe, SceneBloomReduction, SceneBloomTerminal,
    SceneBloomTintPass, SceneBrightCurve, SceneBrightExtraction, SceneGaussianKernelRule,
    SceneGaussianLevel, SceneToneCurveBright,
};
pub use exposure::{SceneExposureAdaptation, ScenePercentileExposure};
mod fxaa;
#[cfg(test)]
mod fxaa_tests;
pub use fxaa::{SceneFxaaKernel, SceneFxaaLuminance, SceneFxaaQuality};
pub use post::{
    SceneAdaptiveExposure, SceneBloom, SceneDisplayGrading, SceneHdrPost, SceneToneMap,
    SceneVignette,
};
mod ssr;
pub use ssr::{
    SceneSsr, SceneSsrComposition, SceneSsrGridDomain, SceneSsrLodMapping, SceneSsrPassBehind,
    SceneSsrPyramid, SceneSsrTraversal,
};
mod error;
mod foliage_wind;
#[cfg(test)]
mod foliage_wind_tests;
mod geometry;
mod scene_geometry;
pub use scene_geometry::SceneGeometry;
mod lod_material;
pub use lod_material::SceneLodMaterialSlot;
mod lod_policy;
pub use lod_policy::SceneFoldSampling;
mod fade_coverage;
mod impostor;
mod material_features;
pub use fade_coverage::{
    SceneFadeCoverage, SceneFadeDirection, SceneFadeQuantization, SceneFadeReference,
    threshold_texels,
};
pub use foliage_wind::{SceneFoliageWind, SceneWindArc, SceneWindBand, SceneWindPhase};
pub use impostor::{
    SceneAtlasSampling, SceneFadeBand, SceneImpostor, SceneImpostorChannels, SceneImpostorLayout,
};
mod translucency;
pub use translucency::{SceneTranslucency, SceneTranslucencyParts};
mod resource;
pub use material_features::{
    SceneAmbientOcclusion, SceneCard, SceneLayerBlend, SceneLayerMode, SceneMaterialFeatures,
    SceneNormalEncoding, SceneNormalY, SceneTerrainContact, SceneTint, SceneWind,
};
mod terrain;
mod terrain_channels;
#[cfg(test)]
mod terrain_channels_tests;
mod terrain_layered;
pub use terrain_channels::{TerrainLayerChannel, TerrainLayerChannelMap, TerrainLayerTexture};
#[cfg(test)]
mod terrain_layered_tests;
pub use terrain_layered::{
    TerrainColorization, TerrainColorizationChannels, TerrainLayer, TerrainLayeredSource,
    TerrainSplatFilter, TerrainTypeGrid,
};
mod terrain_page_policy;
pub use terrain_page_policy::TerrainPagePolicy;
mod terrain_pages;
pub use terrain_pages::{TerrainPageSettings, TerrainPageSettingsError, TerrainPageSettingsParts};
#[cfg(test)]
mod terrain_pages_tests;
pub use composition::{
    DocumentSceneInput, ProvenSceneContribution, RouteDeclarationRef, RouteSceneFactory,
    RouteSceneInput, SceneCompositionFactory, SceneContribution, ScenePlacement,
};
pub use effects::{
    DEFAULT_TAA_CLIP_SCALE_MICRO, EFFECTS_MICRO_SCALE, MAXIMUM_TAA_CLIP_SCALE_MICRO,
    MINIMUM_TAA_CLIP_SCALE_MICRO, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor,
    SceneColorGrading, SceneEffects, SceneEffectsParts, ScenePostStage, SceneShadowCascades,
    SceneSky, SceneSun,
};
pub use error::SceneValidationError;
pub use geometry::{
    BillboardPolicy, MeshHandle, ROTATION_MICRO_SCALE, SceneMesh, SceneMeshLod, SceneObject,
    SceneRotation, SceneTransform, SceneVertex,
};
mod sampler;
pub use resource::{
    MATERIAL_MICRO_SCALE, MaterialHandle, SceneAlphaMode, SceneMaterial, SceneMaterialParts,
    SceneResources, SceneTexture, TexelFormat, TextureColorSpace, TextureEncoding, TextureHandle,
};
pub use sampler::{TextureAnisotropy, TextureFilter, TextureSampler, TextureWrap};
pub use terrain::{
    SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, TerrainLodPolicy,
};
/// Authored parts adopted by [`RenderScene::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneParts {
    /// Texture and material tables.
    pub resources: SceneResources,
    /// Mesh table and ordered objects.
    pub geometry: SceneGeometry,
    /// Optional terrain; its palette resolves against the material table.
    pub terrain: Option<SceneTerrain>,
    /// Optional environmental profile. Absence requests unlit rendering.
    pub effects: Option<SceneEffects>,
}

/// One complete closed renderer-domain scene.
///
/// Instances validate the whole internal closure: object mesh and material
/// handles address their tables, terrain palette handles address the
/// material table, and every child part carries its own validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderScene {
    decals: Box<[SceneDecal]>,
    resources: SceneResources,
    geometry: SceneGeometry,
    terrain: Option<std::sync::Arc<SceneTerrain>>,
    effects: Option<SceneEffects>,
}

impl RenderScene {
    /// Adopts one scene after validating the complete internal closure.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when any child part or cross-part
    /// reference violates its invariant.
    pub fn new(parts: SceneParts) -> Result<Self, SceneValidationError> {
        let scene = Self {
            decals: Box::new([]),
            resources: parts.resources,
            geometry: parts.geometry,
            terrain: parts.terrain.map(std::sync::Arc::new),
            effects: parts.effects,
        };
        scene.validate()?;
        Ok(scene)
    }

    /// Replaces placements and effects while sharing immutable resource tables and terrain.
    ///
    /// # Errors
    /// Returns an error for invalid transforms, effects or cross-table references.
    pub fn with_objects_and_effects(
        &self,
        objects: Vec<SceneObject>,
        effects: Option<SceneEffects>,
    ) -> Result<Self, SceneValidationError> {
        for object in &objects {
            object.transform().validate()?;
        }
        let scene = Self {
            decals: self.decals.clone(),
            resources: self.resources.clone(),
            geometry: SceneGeometry {
                meshes: self.geometry.meshes.clone(),
                objects: objects.into_boxed_slice(),
            },
            terrain: self.terrain.clone(),
            effects,
        };
        scene.validate_references()?;
        Ok(scene)
    }

    /// Replaces resource tables while retaining the immutable terrain allocation.
    ///
    /// # Errors
    /// Returns an error if any existing terrain or object reference becomes invalid.
    pub fn with_resources(
        &self,
        resources: SceneResources,
        geometry: SceneGeometry,
        effects: Option<SceneEffects>,
    ) -> Result<Self, SceneValidationError> {
        let scene = Self {
            decals: self.decals.clone(),
            resources,
            geometry,
            terrain: self.terrain.clone(),
            effects,
        };
        scene.validate()?;
        Ok(scene)
    }

    /// Installs, replaces or removes shared immutable terrain, including its layered source.
    ///
    /// # Errors
    /// Returns an error when terrain or its resource references are invalid.
    pub fn with_terrain(
        &self,
        terrain: Option<std::sync::Arc<SceneTerrain>>,
    ) -> Result<Self, SceneValidationError> {
        let scene = Self {
            terrain,
            ..self.clone()
        };
        scene.validate()?;
        Ok(scene)
    }

    /// Returns the texture and material tables.
    #[must_use]
    pub const fn resources(&self) -> &SceneResources {
        &self.resources
    }

    /// Returns the mesh table and ordered objects.
    #[must_use]
    pub const fn geometry(&self) -> &SceneGeometry {
        &self.geometry
    }

    /// Returns the optional terrain.
    #[must_use]
    pub fn terrain(&self) -> Option<&SceneTerrain> {
        self.terrain.as_deref()
    }

    /// Returns the optional environmental profile.
    #[must_use]
    pub const fn effects(&self) -> Option<&SceneEffects> {
        self.effects.as_ref()
    }

    /// Re-runs the complete internal closure.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.resources.validate()?;
        self.geometry.validate()?;
        if let Some(terrain) = &self.terrain {
            terrain.validate()?;
            if let Some(source) = terrain.layered_source() {
                source.validate_resources(terrain, self.resources().textures())?;
            } else if terrain
                .surfaces()
                .iter()
                .any(|surface| surface.appearance().layered.is_some())
            {
                return Err(SceneValidationError::UnsupportedInput {
                    part: "terrain layers require a layered source",
                });
            }
        }
        self.validate_references()
    }

    fn validate_lod_materials(&self, limit: u64) -> Result<(), SceneValidationError> {
        for lod in self.geometry.meshes().iter().flat_map(SceneMesh::lods) {
            for slot in lod.material_slots().into_iter().flatten() {
                require_handle(
                    "LOD material slot",
                    slot.material().raw(),
                    "material",
                    limit,
                )?;
            }
        }
        Ok(())
    }

    fn validate_references(&self) -> Result<(), SceneValidationError> {
        self.validate_decals()?;
        if let Some(effects) = &self.effects {
            effects.validate()?;
            if let Some(environment) = effects.environment() {
                environment.validate(&self.resources, effects.specular_convention())?;
            }
        }
        if let Some(atmosphere) = self.effects.as_ref().and_then(SceneEffects::atmosphere) {
            atmosphere.validate_handles(
                table_limit(self.resources.textures().len(), "textures")?,
                table_limit(self.geometry.meshes().len(), "meshes")?,
                table_limit(self.resources.materials().len(), "materials")?,
            )?;
        }
        let material_limit = table_limit(self.resources.materials().len(), "materials")?;
        let mesh_limit = table_limit(self.geometry.meshes().len(), "meshes")?;
        if let Some(settings) = self.effects.as_ref().and_then(SceneEffects::water_settings) {
            settings.validate_handles(
                table_limit(self.resources.textures().len(), "textures")?,
                mesh_limit,
            )?;
        }
        if let Some(effects) = &self.effects {
            for body in effects.water_bodies() {
                body.material.validate_handles(
                    table_limit(self.resources.textures().len(), "textures")?,
                    mesh_limit,
                )?;
            }
        }
        self.validate_lod_materials(material_limit)?;
        for object in self.geometry.objects() {
            require_handle("scene object", object.mesh().raw(), "mesh", mesh_limit)?;
            require_handle(
                "scene object",
                object.material().raw(),
                "material",
                material_limit,
            )?;
        }
        if let Some(terrain) = &self.terrain {
            for surface in terrain.surfaces() {
                require_handle(
                    "terrain surfaces",
                    surface.material().raw(),
                    "material",
                    material_limit,
                )?;
            }
        }
        if let Some(crate::scene::effects::SceneSky::DomeTexture { texture }) =
            self.effects.as_ref().map(SceneEffects::sky)
        {
            let texture_limit = table_limit(self.resources.textures().len(), "textures")?;
            require_handle("effects sky", texture.raw(), "texture", texture_limit)?;
        }
        Ok(())
    }
}

/// Converts one table length to an exclusive `u64` bound.
fn table_limit(len: usize, resource: &'static str) -> Result<u64, SceneValidationError> {
    u64::try_from(len).map_err(|_| SceneValidationError::ArithmeticOverflow { part: resource })
}

/// Requires one dense handle value to address its table.
fn require_handle(
    part: &'static str,
    handle: u32,
    resource: &'static str,
    limit: u64,
) -> Result<(), SceneValidationError> {
    if u64::from(handle) < limit {
        Ok(())
    } else {
        Err(SceneValidationError::HandleOutOfRange {
            part,
            resource,
            handle,
            limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::effects::{
        SceneAmbient, SceneAntiAlias, SceneCamera, SceneColorGrading, SceneEffectsParts,
        SceneShadowCascades, SceneSky, SceneSun,
    };
    use crate::scene::geometry::{SceneRotation, SceneVertex};
    use crate::scene::resource::{TexelFormat, TextureColorSpace, TextureEncoding};

    fn texture() -> crate::scene::resource::SceneTexture {
        crate::scene::resource::SceneTexture::Encoded {
            encoding: TextureEncoding::OpaqueExact,
            color_space: TextureColorSpace::Linear,
            bytes: Vec::new().into_boxed_slice(),
        }
    }

    fn effects() -> SceneEffects {
        SceneEffects::new(SceneEffectsParts {
            water: None,
            sky: SceneSky::ProceduralHemisphere {
                zenith: SceneColor {
                    red: 96,
                    green: 148,
                    blue: 220,
                },
                horizon: SceneColor {
                    red: 208,
                    green: 226,
                    blue: 240,
                },
            },
            sun: SceneSun {
                direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
                irradiance_micro: 800_000,
            },
            ambient: SceneAmbient {
                sky_irradiance_micro: 200_000,
                ground_irradiance_micro: 80_000,
            },
            shadows: SceneShadowCascades::new(vec![8_000, 32_000], 10_000)
                .expect("synthetic cascades validate"),
            ssao: Some(SceneSsao {
                receiver: crate::SceneSsaoReceiver::Nearest,
                depth_footprint: crate::SceneSsaoDepthFootprint::default(),
                kernel: crate::SceneSsaoKernel::Hemisphere,
                filter: crate::SceneSsaoFilter::DepthAwareBox,
                sample_count: 16,
                bias_millimeters: 10,
                radius_millimeters: 1_200,
                intensity_micro: 1_000_000,
                horizon_angle_millidegrees: 90_000,
            }),
            camera: SceneCamera {
                vertical_fov_millidegrees: 60_000,
                near_plane_millimeters: 100,
                far_plane_millimeters: 2_000_000,
            },
            grading: SceneColorGrading {
                exposure_micro: 1_000_000,
                contrast_micro: 1_000_000,
                saturation_micro: 1_000_000,
            },
            post: Vec::new(),
            anti_alias: SceneAntiAlias::Disabled,
        })
        .expect("synthetic effects validate")
    }

    fn mesh() -> SceneMesh {
        let vertex = SceneVertex {
            tangent_micro: None,
            position_millimeters: [0, 0, 0],
            normal_micro: [0, 1_000_000, 0],
            texcoord_micro: [0, 0],
        };
        SceneMesh::new(vec![
            SceneMeshLod::new(0, vec![vertex], vec![0, 0, 0], None)
                .expect("synthetic lod validates"),
        ])
        .expect("synthetic mesh validates")
    }

    fn scene_with(
        objects: Vec<SceneObject>,
        terrain: Option<crate::scene::terrain::SceneTerrain>,
    ) -> Result<RenderScene, SceneValidationError> {
        let material = SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: None,
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 500_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: false,
        })
        .expect("synthetic material validates");
        let resources = SceneResources::new(vec![texture()], vec![material])
            .expect("synthetic resources validate");
        RenderScene::new(SceneParts {
            resources,
            geometry: SceneGeometry::new(vec![mesh()], objects)?,
            terrain,
            effects: Some(effects()),
        })
    }

    fn object(mesh: u32, material: u32) -> SceneObject {
        SceneObject::new(
            SceneTransform {
                position_millimeters: [0, 0, 0],
                rotation: SceneRotation::IDENTITY,
                scale_millionths: 1_000_000,
            },
            MeshHandle::from_raw(mesh),
            MaterialHandle::from_raw(material),
        )
    }

    fn terrain(materials: Vec<MaterialHandle>) -> crate::scene::terrain::SceneTerrain {
        let surfaces = materials
            .into_iter()
            .map(|material| SceneTerrainSurface::new(material, SceneTerrainAppearance::NEUTRAL))
            .collect();
        crate::scene::terrain::SceneTerrain::new(crate::scene::terrain::SceneTerrainParts {
            columns: 2,
            rows: 2,
            origin_east_millimeters: 0,
            origin_north_millimeters: 0,
            sample_spacing_millimeters: 600,
            heights_millimeters: vec![0; 4],
            surface_indices: vec![0; 4],
            surfaces,
            lod_policy: TerrainLodPolicy::FullResolution,
        })
        .expect("synthetic terrain validates")
    }

    #[test]
    fn placement_edits_share_resource_generations_and_validate_new_references() {
        let scene = scene_with(
            vec![object(0, 0)],
            Some(terrain(vec![MaterialHandle::from_raw(0)])),
        )
        .unwrap();
        let edited = scene
            .with_objects_and_effects(vec![object(0, 0)], scene.effects().cloned())
            .unwrap();
        assert_eq!(
            scene.resources().textures().as_ptr(),
            edited.resources().textures().as_ptr()
        );
        assert_eq!(
            scene.resources().materials().as_ptr(),
            edited.resources().materials().as_ptr()
        );
        assert_eq!(
            scene.geometry().meshes().as_ptr(),
            edited.geometry().meshes().as_ptr()
        );
        assert!(std::ptr::eq(
            scene.terrain().unwrap(),
            edited.terrain().unwrap()
        ));
        assert!(
            scene
                .with_objects_and_effects(vec![object(1, 0)], None)
                .is_err()
        );
        assert!(
            scene
                .with_objects_and_effects(vec![object(0, 1)], None)
                .is_err()
        );
    }

    #[test]
    fn object_handles_must_address_their_tables() {
        assert!(scene_with(vec![object(0, 0)], None).is_ok());
        assert!(matches!(
            scene_with(vec![object(1, 0)], None),
            Err(SceneValidationError::HandleOutOfRange {
                resource: "mesh",
                handle: 1,
                ..
            })
        ));
        assert!(matches!(
            scene_with(vec![object(0, 1)], None),
            Err(SceneValidationError::HandleOutOfRange {
                resource: "material",
                handle: 1,
                ..
            })
        ));
    }

    #[test]
    fn terrain_palette_must_address_the_material_table() {
        assert!(scene_with(Vec::new(), Some(terrain(vec![MaterialHandle::from_raw(0)])),).is_ok());
        assert!(matches!(
            scene_with(
                Vec::new(),
                Some(terrain(vec![
                    MaterialHandle::from_raw(0),
                    MaterialHandle::from_raw(1)
                ])),
            ),
            Err(SceneValidationError::HandleOutOfRange {
                resource: "material",
                handle: 1,
                ..
            })
        ));
    }

    #[test]
    fn dome_sky_texture_must_address_the_texture_table() {
        let parts = SceneParts {
            resources: SceneResources::new(Vec::new(), Vec::new()).expect("empty tables validate"),
            geometry: SceneGeometry::new(Vec::new(), Vec::new()).expect("empty geometry validates"),
            terrain: None,
            effects: Some(dome_sky_effects()),
        };
        assert!(matches!(
            RenderScene::new(parts),
            Err(SceneValidationError::HandleOutOfRange {
                resource: "texture",
                handle: 0,
                ..
            })
        ));
    }

    fn dome_sky_effects() -> SceneEffects {
        let base = effects();
        SceneEffects::new(SceneEffectsParts {
            water: None,
            sky: SceneSky::DomeTexture {
                texture: TextureHandle::from_raw(0),
            },
            sun: *base.sun(),
            ambient: *base.ambient(),
            shadows: base.shadows().clone(),
            ssao: base.ssao().copied(),
            camera: *base.camera(),
            grading: *base.grading(),
            post: Vec::new(),
            anti_alias: SceneAntiAlias::Disabled,
        })
        .expect("dome sky effects validate")
    }

    #[test]
    fn empty_tables_are_representable_without_references() {
        let material = SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: None,
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 500_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: false,
        })
        .expect("synthetic material validates");
        let scene = RenderScene::new(SceneParts {
            resources: SceneResources::new(
                vec![crate::scene::resource::SceneTexture::Texels {
                    format: TexelFormat::R8Unorm,
                    color_space: TextureColorSpace::Linear,
                    width: 1,
                    height: 1,
                    mips: 1,
                    bytes: vec![0].into_boxed_slice(),
                }],
                vec![material],
            )
            .expect("resources validate"),
            geometry: SceneGeometry::new(Vec::new(), Vec::new()).expect("empty geometry validates"),
            terrain: None,
            effects: Some(effects()),
        })
        .expect("scene without objects validates");
        assert_eq!(scene.geometry().objects().len(), 0);
        assert!(scene.terrain().is_none());
    }
}
mod wind_field;
pub use wind_field::{ScenePlanarWindField, SceneStaticWindModifier};
mod terrain_streaming;
pub use terrain_streaming::TerrainStreamingPolicy;
