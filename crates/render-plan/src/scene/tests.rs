use super::*;
use crate::scene::effects::{
    SceneAmbient, SceneAntiAlias, SceneCamera, SceneColorGrading, SceneEffectsParts,
    SceneShadowCascades, SceneSky, SceneSun,
};
use crate::scene::geometry::{SceneRotation, SceneVertex};
use crate::scene::resource::{TexelFormat, TextureColorSpace, TextureEncoding};

pub(super) fn texture() -> crate::scene::resource::SceneTexture {
    crate::scene::resource::SceneTexture::Encoded {
        encoding: TextureEncoding::OpaqueExact,
        color_space: TextureColorSpace::Linear,
        bytes: Vec::new().into_boxed_slice(),
    }
}

pub(super) fn effects() -> SceneEffects {
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
        SceneMeshLod::new(0, vec![vertex], vec![0, 0, 0], None).expect("synthetic lod validates"),
    ])
    .expect("synthetic mesh validates")
}

pub(super) fn scene_with(
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
    let resources =
        SceneResources::new(vec![texture()], vec![material]).expect("synthetic resources validate");
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![mesh()], objects)?,
        terrain,
        effects: Some(effects()),
    })
}

pub(super) fn object(mesh: u32, material: u32) -> SceneObject {
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

pub(super) fn terrain(materials: Vec<MaterialHandle>) -> crate::scene::terrain::SceneTerrain {
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
fn contour_references_are_checked_at_every_scene_replacement() {
    let scene = scene_with(vec![object(0, 0)], None).unwrap();
    let selected = object(0, 0).with_contour_id(255);
    assert!(
        scene
            .with_objects_and_effects(vec![selected], None)
            .is_err()
    );
    assert!(
        scene
            .with_objects_and_effects(vec![selected], Some(effects()))
            .is_err()
    );
    let mut styles = vec![None; 256];
    styles[255] = Some(SceneContourStyle::default());
    let defined = effects()
        .with_contours(SceneContours::new([1920, 1080], styles.clone()).unwrap())
        .unwrap();
    let selected_scene = scene
        .with_objects_and_effects(vec![selected], Some(defined))
        .unwrap();
    assert!(selected_scene.validate().is_ok());
    assert!(
        selected_scene
            .with_resources(
                selected_scene.resources().clone(),
                selected_scene.geometry().clone(),
                None
            )
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

pub(super) fn dome_sky_effects() -> SceneEffects {
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

#[test]
fn contour_ids_require_styles_at_whole_scene_and_replacement_boundaries() {
    let selected = object(0, 0).with_contour_id(255);
    assert!(scene_with(vec![selected], None).is_err());
    let base = scene_with(vec![object(0, 0)], None).unwrap();
    let mut styles = vec![None; 256];
    styles[255] = Some(SceneContourStyle::default());
    let effects = effects()
        .with_contours(SceneContours::new([1920, 1080], styles).unwrap())
        .unwrap();
    let selected_scene = base
        .with_objects_and_effects(vec![selected], Some(effects))
        .unwrap();
    assert!(selected_scene.validate().is_ok());
    assert!(base.with_objects_and_effects(vec![selected], None).is_err());
    assert!(
        base.with_objects_and_effects(vec![selected], Some(self::effects()))
            .is_err()
    );
    assert!(
        base.with_objects_and_effects(
            vec![selected.with_contour_id(1)],
            selected_scene.effects().cloned()
        )
        .is_err()
    );
    assert!(
        selected_scene
            .with_resources(
                selected_scene.resources().clone(),
                selected_scene.geometry().clone(),
                Some(self::effects()),
            )
            .is_err()
    );
    assert!(
        selected_scene
            .with_objects_and_effects(vec![selected.with_contour_id(0)], None,)
            .is_ok()
    );
}
