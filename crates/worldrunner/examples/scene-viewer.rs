use worldrunner::scene::{
    gpu::{CameraPose, shadows::DirectionalShadowSettings},
    plan::{
        MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode,
        SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading, SceneEffects,
        SceneEffectsParts, SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh,
        SceneMeshLod, SceneObject, SceneParts, SceneResources, SceneShadowCascades, SceneSky,
        SceneSun, SceneTexture, SceneTransform, SceneVertex, TexelFormat, TextureColorSpace,
        TextureHandle,
    },
    viewer::{OrbitLimits, ViewerOptions, run},
};
use worldrunner_render_plan::{
    SceneDirection, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy,
};
const MICRO: i64 = 1_000_000;

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn effects(terrain: bool) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: SceneSun {
            direction: SceneDirection::new(if terrain {
                [600_000, 0, 800_000]
            } else {
                [0, 0, 1_000_000]
            })
            .unwrap(),
            irradiance_micro: if terrain { 1_000_000 } else { 0 },
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: if terrain { 100_000 } else { 1_000_000 },
            ground_irradiance_micro: if terrain { 100_000 } else { 1_000_000 },
        },
        shadows: SceneShadowCascades::new(vec![2_000, 4_000, 8_000, 16_000], 100_000)
            .expect("four coverage cascades are valid"),
        ssao: None,
        camera: camera(),
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    })
    .expect("the example effects are valid")
}

fn identity() -> SceneTransform {
    SceneTransform::from_world_schema(
        worldrunner::scene::plan::world_schema::WorldCoordinateFrame::RightHandedYUp,
        [0; 3],
        [0, 0, 0, ROTATION_MICRO_SCALE],
        1_000_000,
    )
    .expect("the example placement uses the schema boundary")
}

fn vertex(position: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex::from_world_schema(
        worldrunner::scene::plan::world_schema::WorldCoordinateFrame::RightHandedYUp,
        position,
        [0, MICRO, 0],
        texcoord,
    )
    .expect("the example mesh uses the schema boundary")
}

/// A two-metre ground quad at elevation zero, facing up.
///
/// The winding is counter-clockwise seen from above, which is what the
/// pipelines declare as front-facing. A quad wound the other way would vanish
/// entirely under back-face culling.
fn quad() -> SceneMesh {
    let half = 1_000;
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-half, 0, half], [0, 0]),
                vertex([half, 0, half], [MICRO, 0]),
                vertex([half, 0, -half], [MICRO, MICRO]),
                vertex([-half, 0, -half], [0, MICRO]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .expect("the example quad is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

fn material(alpha_mode: SceneAlphaMode, double_sided: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided,
    })
    .expect("the example material carries in-range scalars")
}

fn terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 7,
        rows: 7,
        origin_east_millimeters: -3_000,
        origin_north_millimeters: -3_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: (0..49)
            .map(|i| if i % 7 == 4 { 2_000 } else { 0 })
            .collect(),
        surface_indices: vec![0; 49],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(1),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn scene(with_terrain: bool) -> RenderScene {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255, 0, 0, 255].into(),
    };
    let map = |bytes: [u8; 4]| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Vec::from(bytes).into(),
    };
    let terrain_material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let terrain = with_terrain.then(terrain);
    let mut placement = identity();
    if with_terrain {
        placement.position_millimeters[2] = 1_500;
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture, map([128, 128, 255, 255]), map([0, 255, 0, 255])],
            vec![material(SceneAlphaMode::Opaque, false), terrain_material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad()],
            vec![SceneObject::new(
                placement,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain,
        effects: Some(effects(with_terrain)),
    })
    .unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let terrain = std::env::args().any(|arg| arg == "--terrain");
    let shadows =
        std::env::args()
            .any(|arg| arg == "--shadows")
            .then_some(DirectionalShadowSettings {
                focus_meters: worldrunner::glam::Vec3::ZERO,
                half_extent_meters: 4.0,
                depth_half_extent_meters: 4.0,
                resolution: 512,
                depth_bias: 0.001,
                normal_bias_meters: 0.01,
            });
    run(
        scene(terrain),
        ViewerOptions {
            camera: SceneCamera {
                vertical_fov_millidegrees: 60_000,
                near_plane_millimeters: 100,
                far_plane_millimeters: 1_000_000,
            },
            title: "Generic scene viewer".into(),
            size: [800, 600],
            pose: CameraPose {
                eye_millimeters: if terrain {
                    [0, -7_000, 6_000]
                } else {
                    [0, -2000, 2000]
                },
                target_millimeters: [0, 0, 0],
            },
            orbit: OrbitLimits {
                min_distance_millimeters: 100.0,
                max_distance_millimeters: 10_000.0,
                min_elevation_radians: -1.4,
                max_elevation_radians: 1.4,
            },
            shadows,
        },
    )?;
    Ok(())
}
