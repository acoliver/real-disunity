use worldrunner_render_plan::*;
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

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
            direction: SceneDirection::new([600_000, 0, 800_000]).unwrap(),
            irradiance_micro: 1_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 100_000,
            ground_irradiance_micro: 100_000,
        },
        shadows: SceneShadowCascades::new(vec![4_000], 0).unwrap(),
        ssao: None,
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 20_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: SceneAntiAlias::taa(900_000),
    })
    .unwrap()
    .with_ssr(SceneSsr {
        enabled: true,
        max_steps: 256,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 1_000_000,
        ..Default::default()
    })
    .unwrap()
}

fn material(texture: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(texture),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
    .with_features(SceneMaterialFeatures {
        tint: SceneTint::WHITE,
        ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
        wind: Some(
            SceneWind::new(
                SceneDirection::new([1_000_000, 0, 0]).unwrap(),
                500,
                1000,
                0,
            )
            .unwrap(),
        ),
        layer: None,
        translucency: None,
    })
    .unwrap()
}

fn quad(half: i64) -> SceneMesh {
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            [
                ([-half, -half, 0], [0, 0]),
                ([half, -half, 0], [1_000_000, 0]),
                ([half, half, 0], [1_000_000, 1_000_000]),
                ([-half, half, 0], [0, 1_000_000]),
            ]
            .into_iter()
            .map(|(position, uv)| SceneVertex {
                tangent_micro: None,
                position_millimeters: position,
                normal_micro: [0, 0, 1_000_000],
                texcoord_micro: uv,
            })
            .collect(),
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap()
}

fn object(position: [i64; 3], mesh: u32) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: position,
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 0,
                w: ROTATION_MICRO_SCALE,
            },
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(mesh),
    )
}

pub(super) fn source() -> RenderScene {
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![220, 220, 220, 255].into(),
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material(0), material(0)]).unwrap(),
        geometry: SceneGeometry::new(
            vec![quad(3000), quad(300)],
            vec![
                object([0, 0, 0], 0),
                object([0, 0, 1000], 1),
                object([400, 500, 800], 1),
            ],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

pub(super) fn view(source: &RenderScene) -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [0, -4000, 6500],
            target_millimeters: [0, 0, 0],
        },
        *source.effects().unwrap().camera(),
        Viewport {
            width: 257,
            height: 241,
        },
    )
    .unwrap()
}
