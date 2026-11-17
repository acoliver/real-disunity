use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode, SceneAmbient,
    SceneAtmosphere, SceneBackgroundDepthPolicy, SceneCamera, SceneCelestialComposition,
    SceneColorGrading, SceneDirection, SceneEffects, SceneEffectsParts, SceneGeometry,
    SceneImpostor, SceneImpostorLayout, SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod,
    SceneObject, SceneParts, SceneResources, SceneRotation, SceneShadowCascades, SceneSky,
    SceneSsao, SceneSsaoDepthFootprint, SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver,
    SceneSun, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    SceneTexture, SceneTransform, SceneVertex, TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

pub const SIZE: [u32; 2] = [257, 241];

pub fn effects() -> SceneEffects {
    let parts = SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: worldrunner_render_plan::SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: worldrunner_render_plan::SceneColor {
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
        ssao: Some(ssao()),
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
        anti_alias: worldrunner_render_plan::SceneAntiAlias::Disabled,
    };
    SceneEffects::new(parts)
        .unwrap()
        .with_atmosphere(sky())
        .unwrap()
}

fn ssao() -> SceneSsao {
    SceneSsao {
        receiver: SceneSsaoReceiver::Nearest,
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2_000,
        intensity_micro: 1_000_000,
        sample_count: 32,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}

fn sky() -> SceneAtmosphere {
    SceneAtmosphere {
        celestial_composition: SceneCelestialComposition::Inherit,
        celestial_exposure_micro: None,
        background_depth_policy: SceneBackgroundDepthPolicy::ConstantDistance,
        scattering: None,
        horizon_radiance_micro: [200_000, 300_000, 2_000_000],
        zenith_radiance_micro: [100_000, 200_000, 2_000_000],
        gradient_exponent_micro: 1_000_000,
        sun_angular_radius_millidegrees: 0,
        fog: None,
        discs: Vec::new(),
        meshes: Vec::new(),
        background_distance_millimeters: 20_000,
    }
}

fn base_texture() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 4,
        height: 1,
        mips: 1,
        bytes: [[220, 220, 220, 255]; 4]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .into(),
    }
}

fn terrain_map(bytes: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Vec::from(bytes).into_boxed_slice(),
    }
}

fn terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 7,
        rows: 7,
        origin_east_millimeters: -3_000,
        origin_north_millimeters: -3_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: vec![0; 49],
        surface_indices: vec![0; 49],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(2),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: worldrunner_render_plan::TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn card_mesh() -> SceneMesh {
    let corners = [
        ([-300, 0, -300], [0, 0]),
        ([300, 0, -300], [1_000_000, 0]),
        ([300, 0, 300], [1_000_000, 1_000_000]),
        ([-300, 0, 300], [0, 1_000_000]),
    ];
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            corners
                .into_iter()
                .map(|(position, texcoord_micro)| SceneVertex {
                    tangent_micro: None,
                    position_millimeters: position,
                    normal_micro: [0, -1_000_000, 0],
                    texcoord_micro,
                })
                .collect(),
            vec![0, 1, 2, 0, 2, 3],
            Some(worldrunner_render_plan::BillboardPolicy::YawLocked),
        )
        .unwrap(),
    ])
    .unwrap()
}

fn object(position: [i64; 3], mesh: u32, material: u32) -> SceneObject {
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
        MaterialHandle::from_raw(material),
    )
}

fn card_material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(4)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap(),
    )
}

pub fn source() -> RenderScene {
    let mut textures = vec![base_texture(), base_texture()];
    textures.extend([
        terrain_map([128, 128, 255, 255]),
        terrain_map([0, 255, 0, 255]),
        terrain_map([128, 128, 0, 255]),
        SceneTexture::Texels {
            format: TexelFormat::Rg8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 4,
            height: 1,
            mips: 1,
            bytes: vec![32, 128, 64, 128, 192, 128, 224, 128].into_boxed_slice(),
        },
    ]);
    let terrain_material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(2)),
        roughness_metallic: Some(TextureHandle::from_raw(3)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let opaque = |texture| {
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
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            textures,
            vec![opaque(0), opaque(1), terrain_material, card_material()],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad(3_000), quad(300), card_mesh()],
            vec![
                object([0, 0, 1_000], 1, 1),
                object([-1_500, 500, 300], 2, 3),
            ],
        )
        .unwrap(),
        terrain: Some(terrain()),
        effects: Some(effects()),
    })
    .unwrap()
}

fn quad(half: i64) -> SceneMesh {
    let corners = [
        ([-half, -half, 0], [0, 0]),
        ([half, -half, 0], [1_000_000, 0]),
        ([half, half, 0], [1_000_000, 1_000_000]),
        ([-half, half, 0], [0, 1_000_000]),
    ];
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            corners
                .into_iter()
                .map(|(position_millimeters, texcoord_micro)| SceneVertex {
                    tangent_micro: None,
                    position_millimeters,
                    normal_micro: [0, 0, 1_000_000],
                    texcoord_micro,
                })
                .collect(),
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .unwrap(),
    ])
    .unwrap()
}

pub fn view(index: usize) -> SceneView {
    let eye_x = -400 + 800 * i64::try_from(index).unwrap() / 149;
    SceneView::new(
        CameraPose {
            eye_millimeters: [eye_x, -4_000, 6_500],
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
