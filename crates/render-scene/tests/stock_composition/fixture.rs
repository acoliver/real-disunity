//! The synthetic scene the composition tests draw: one lit quad under screen-space metadata.
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode, SceneAmbient,
    SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading, SceneDirection, SceneEffects,
    SceneEffectsParts, SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod,
    SceneObject, SceneParts, SceneResources, SceneRotation, SceneShadowCascades, SceneSky,
    SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver,
    SceneSun, SceneTexture, SceneTransform, SceneVertex, TexelFormat, TextureColorSpace,
    TextureHandle,
};
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

pub const SIZE: [u32; 2] = [64, 48];

pub fn effects() -> SceneEffects {
    effects_with(SceneAntiAlias::Disabled)
}

pub fn effects_with(anti_alias: SceneAntiAlias) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 120,
                green: 160,
                blue: 255,
            },
            horizon: SceneColor {
                red: 220,
                green: 230,
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
        ssao: Some(SceneSsao {
            receiver: SceneSsaoReceiver::Nearest,
            depth_footprint: SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Hemisphere,
            filter: SceneSsaoFilter::DepthAwareBox,
            radius_millimeters: 2000,
            intensity_micro: 1_000_000,
            sample_count: 16,
            bias_millimeters: 10,
            horizon_angle_millidegrees: 90_000,
        }),
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
        anti_alias,
    })
    .unwrap()
}

/// One lit quad facing the camera.
pub fn scene() -> RenderScene {
    scene_with(effects())
}

fn scene_with(effects: SceneEffects) -> RenderScene {
    let half = 3_000;
    let corners = [
        ([-half, -half, 0], [0, 0]),
        ([half, -half, 0], [1_000_000, 0]),
        ([half, half, 0], [1_000_000, 1_000_000]),
        ([-half, half, 0], [0, 1_000_000]),
    ];
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            corners
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
    .unwrap();
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 4,
        height: 1,
        mips: 1,
        bytes: [[200, 120, 60, 255]; 4]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .into(),
    };
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0, 0, 0],
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 0,
                w: ROTATION_MICRO_SCALE,
            },
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material]).unwrap(),
        geometry: SceneGeometry::new(vec![mesh], vec![object]).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

pub fn view(eye_x: i64) -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -2_000, 5_500],
            target_millimeters: [0, 0, 0],
        },
        *effects().camera(),
        Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap()
}
