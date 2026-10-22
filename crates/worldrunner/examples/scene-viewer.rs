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
use worldrunner_render_plan::SceneDirection;
const MICRO: i64 = 1_000_000;

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn effects() -> SceneEffects {
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
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            // Ambient-only lighting makes channel-order errors obvious in this small example.
            irradiance_micro: 0,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
        shadows: SceneShadowCascades::new(vec![10_000], 0).expect("one unblended cascade is valid"),
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

fn scene() -> RenderScene {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255, 0, 0, 255].into(),
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad()],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        scene(),
        ViewerOptions {
            camera: SceneCamera {
                vertical_fov_millidegrees: 60_000,
                near_plane_millimeters: 100,
                far_plane_millimeters: 1_000_000,
            },
            title: "Generic scene viewer".into(),
            size: [800, 600],
            pose: CameraPose {
                eye_millimeters: [0, -2000, 2000],
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
