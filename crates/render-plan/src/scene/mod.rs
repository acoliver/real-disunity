//! Closed owned renderer-domain scene model.
//!
//! A [`RenderScene`] is wholly owned CPU state: typed dense handles, owned
//! boxed payloads, and closed renderer-semantic variants. The module carries
//! no paths, no package, profile, or dataset identifiers, no role URI
//! interpretation, no `Default`, no fallback, and no data borrowed from a
//! publication. Every constructor validates its own closure, so an invalid
//! scene cannot exist.

mod direction;
pub use direction::{DIRECTION_MICRO_SCALE, SceneDirection};
mod composition;
mod effects;
mod water;
mod water_settings;
pub use water::SceneWaterBody;
pub use water_settings::{SceneWaterGeometry, SceneWaterSettings};
mod post;
pub use post::{SceneAdaptiveExposure, SceneBloom, SceneHdrPost};
mod ssr;
pub use ssr::SceneSsr;
mod error;
mod geometry;
mod material_features;
mod resource;
pub use material_features::{
    SceneAmbientOcclusion, SceneCard, SceneLayerBlend, SceneLayerMode, SceneMaterialFeatures,
    SceneTint, SceneWind,
};
mod terrain;

pub use composition::{
    DocumentSceneInput, ProvenSceneContribution, RouteDeclarationRef, RouteSceneFactory,
    RouteSceneInput, SceneCompositionFactory, SceneContribution, ScenePlacement,
};
pub use effects::{
    EFFECTS_MICRO_SCALE, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading,
    SceneEffects, SceneEffectsParts, ScenePostStage, SceneShadowCascades, SceneSky, SceneSsao,
    SceneSun,
};
pub use error::SceneValidationError;
pub use geometry::{
    BillboardPolicy, MeshHandle, ROTATION_MICRO_SCALE, SceneMesh, SceneMeshLod, SceneObject,
    SceneRotation, SceneTransform, SceneVertex,
};
pub use resource::{
    MATERIAL_MICRO_SCALE, MaterialHandle, SceneAlphaMode, SceneMaterial, SceneMaterialParts,
    SceneResources, SceneTexture, TexelFormat, TextureColorSpace, TextureEncoding, TextureHandle,
};
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

/// Mesh table plus ordered objects of one scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneGeometry {
    meshes: Box<[SceneMesh]>,
    objects: Box<[SceneObject]>,
}

impl SceneGeometry {
    /// Adopts the mesh table and ordered objects.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when a mesh chain or an object
    /// transform violates its own closure.
    pub fn new(
        meshes: Vec<SceneMesh>,
        objects: Vec<SceneObject>,
    ) -> Result<Self, SceneValidationError> {
        let geometry = Self {
            meshes: meshes.into_boxed_slice(),
            objects: objects.into_boxed_slice(),
        };
        geometry.validate()?;
        Ok(geometry)
    }

    /// Returns the owned mesh table.
    #[must_use]
    pub fn meshes(&self) -> &[SceneMesh] {
        &self.meshes
    }

    /// Returns the ordered objects in authored draw order.
    #[must_use]
    pub fn objects(&self) -> &[SceneObject] {
        &self.objects
    }

    /// Validates every mesh chain and object transform.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for mesh in &self.meshes {
            mesh.validate()?;
        }
        for object in &self.objects {
            object.transform().validate()?;
        }
        Ok(())
    }
}

/// One complete closed renderer-domain scene.
///
/// Instances validate the whole internal closure: object mesh and material
/// handles address their tables, terrain palette handles address the
/// material table, and every child part carries its own validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderScene {
    resources: SceneResources,
    geometry: SceneGeometry,
    terrain: Option<SceneTerrain>,
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
            resources: parts.resources,
            geometry: parts.geometry,
            terrain: parts.terrain,
            effects: parts.effects,
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
    pub const fn terrain(&self) -> Option<&SceneTerrain> {
        self.terrain.as_ref()
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
        if let Some(effects) = &self.effects {
            effects.validate()?;
        }
        let material_limit = table_limit(self.resources.materials().len(), "materials")?;
        let mesh_limit = table_limit(self.geometry.meshes().len(), "meshes")?;
        if let Some(settings) = self.effects.as_ref().and_then(SceneEffects::water_settings) {
            settings.validate_handles(
                table_limit(self.resources.textures().len(), "textures")?,
                mesh_limit,
            )?;
        }
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
            terrain.validate()?;
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
        SceneShadowCascades, SceneSky, SceneSsao, SceneSun,
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
